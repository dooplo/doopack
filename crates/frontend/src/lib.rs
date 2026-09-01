mod api;

use leptos::prelude::*;
use shared::*;
use wasm_bindgen::prelude::*;
use gloo_timers::future::TimeoutFuture;
use wasm_bindgen_futures::spawn_local;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ActivePage {
    Login,
    Dashboard,
    SystemHealth,
    // Events
    Bindings,
    Services,
    Queues,
    Connections, // DB Pool
    Schedules,
    // Flows
    Logs,
    Flow,        // Views
    // Developer
    ApiKeys,
    Docs,
    SdkDoc,
    // Backup
    Backup,
}

#[component]
pub fn App() -> impl IntoView {
    let has_token = if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.local_storage() {
            storage.get_item("auth_token").ok().flatten().is_some()
        } else {
            false
        }
    } else {
        false
    };

    let active_page = RwSignal::new(if has_token { ActivePage::Dashboard } else { ActivePage::Login });
    provide_context(active_page);
    let logged_in = RwSignal::new(has_token);
    let auth_error = RwSignal::new(None::<String>);
    let email_input = RwSignal::new(String::new());
    let password_input = RwSignal::new(String::new());

    let do_submit_login = move || {
        let email = email_input.get();
        let password = password_input.get();
        auth_error.set(None);
        spawn_local(async move {
            let payload = LoginRequest { email: email.clone(), password: password.clone() };
            match api::login(payload).await {
                Ok(_) => {
                    logged_in.set(true);
                    active_page.set(ActivePage::Dashboard);
                }
                Err(e) => {
                    auth_error.set(Some(format!("Auth failed: {}", e)));
                }
            }
        });
    };

    // Global listeners: session expiration & Esc key
    if let Some(window) = web_sys::window() {
        use wasm_bindgen::JsCast;
        let logged_in_c = logged_in.clone();
        let active_page_c = active_page.clone();
        let auth_error_c = auth_error.clone();
        let cb_unauth = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            logged_in_c.set(false);
            active_page_c.set(ActivePage::Login);
            auth_error_c.set(Some("Sua sessão expirou. Por favor, faça login novamente.".to_string()));
        });
        let _ = window.add_event_listener_with_callback("auth_unauthorized", cb_unauth.as_ref().unchecked_ref());
        cb_unauth.forget();

        let cb_esc = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |ev: web_sys::KeyboardEvent| {
            if ev.key() == "Escape" {
                if let Some(w) = web_sys::window() {
                    if let Ok(event) = web_sys::CustomEvent::new("close_all_modals") {
                        let _ = w.dispatch_event(&event);
                    }
                }
            }
        });
        let _ = window.add_event_listener_with_callback("keydown", cb_esc.as_ref().unchecked_ref());
        cb_esc.forget();
    }

    view! {
        <div class="min-h-screen bg-slate-50 text-slate-800 flex flex-col font-sans">
            {move || {
                if !logged_in.get() && active_page.get() == ActivePage::Login {
                    view! {
                        <div class="flex-1 flex items-center justify-center p-6 bg-slate-100">
                            <div class="max-w-md w-full bg-white border border-slate-200 rounded-xl p-8 shadow-xl">
                                <div class="text-center mb-8 flex flex-col items-center justify-center">
                                    <img src="/logo.png?v=12" class="px-4 object-contain mb-3" />
                                </div>

                                {move || auth_error.get().map(|err| view! {
                                    <div class="mb-4 p-3 bg-red-50 border border-red-200 text-red-700 rounded-lg text-sm">
                                        {err}
                                    </div>
                                })}

                                <div class="space-y-4">
                                    <div>
                                        <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">"Email Address"</label>
                                        <input
                                            type="email"
                                            placeholder="admin@orchestrator.com"
                                            class="w-full bg-slate-50 border border-slate-300 focus:border-slate-950 focus:ring-1 focus:ring-slate-950 rounded-lg px-4 py-2.5 text-slate-900 transition duration-200 placeholder-slate-400 outline-none"
                                            on:input=move |ev| email_input.set(event_target_value(&ev))
                                            on:keydown=move |ev| {
                                                if ev.key() == "Enter" {
                                                    do_submit_login();
                                                }
                                            }
                                        />
                                    </div>

                                    <div>
                                        <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">"Password"</label>
                                        <input
                                            type="password"
                                            placeholder="••••••••"
                                            class="w-full bg-slate-50 border border-slate-300 focus:border-slate-950 focus:ring-1 focus:ring-slate-950 rounded-lg px-4 py-2.5 text-slate-900 transition duration-200 placeholder-slate-400 outline-none"
                                            on:input=move |ev| password_input.set(event_target_value(&ev))
                                            on:keydown=move |ev| {
                                                if ev.key() == "Enter" {
                                                    do_submit_login();
                                                }
                                            }
                                        />
                                    </div>

                                    <button
                                        class="w-full bg-slate-950 hover:bg-black text-white font-semibold py-3 px-4 rounded-lg transition duration-200 shadow-md shadow-slate-950/10 active:translate-y-px"
                                        on:click=move |_| {
                                            do_submit_login();
                                        }
                                    >
                                        "Sign In"
                                    </button>
                                </div>
                            </div>
                        </div>
                    }.into_any()
                } else {
                    let do_export_backup = move || {
                        spawn_local(async move {
                            match api::export_system_data().await {
                                Ok(json_val) => {
                                    if let Some(window) = web_sys::window() {
                                        if let Some(document) = window.document() {
                                            let json_str = serde_json::to_string_pretty(&json_val).unwrap_or_default();
                                            let bag = web_sys::BlobPropertyBag::new();
                                            bag.set_type("application/json");
                                            let array = js_sys::Array::new();
                                            array.push(&wasm_bindgen::JsValue::from_str(&json_str));
                                            match web_sys::Blob::new_with_str_sequence_and_options(&array, &bag) {
                                                Ok(blob) => {
                                                    if let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) {
                                                        if let Ok(a) = document.create_element("a") {
                                                            let a_el: web_sys::HtmlAnchorElement = a.unchecked_into();
                                                            a_el.set_href(&url);
                                                            let date_str = js_sys::Date::new_0().to_iso_string().as_string().unwrap_or_else(|| "backup".to_string());
                                                            a_el.set_download(&format!("doopack_backup_{}.json", date_str.replace(":", "-")));
                                                            a_el.click();
                                                            let _ = web_sys::Url::revoke_object_url(&url);
                                                        }
                                                    }
                                                }
                                                Err(e) => {
                                                    let _ = window.alert_with_message(&format!("Failed to create blob: {:?}", e));
                                                }
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    if let Some(w) = web_sys::window() {
                                        let _ = w.alert_with_message(&format!("Export failed: {}", e));
                                    }
                                }
                            }
                        });
                    };

                    view! {
                        <div class="flex-1 flex overflow-hidden">
                            // Navigation Sidebar (Light Theme with Grouped Sections)
                            <aside class="w-64 bg-white border-r border-slate-200 flex flex-col select-none">
                                <div class="h-16 flex items-center px-6 py-2 gap-2 border-b border-slate-100">
                                    <img src="/logo.png" class="object-contain item-center" style="height: 52px;"/>
                                </div>
                                <nav class="flex-1 p-3 space-y-4 overflow-y-auto">
                                    // 1. DASHBOARD
                                    <div>
                                        <button
                                            class=move || format!("w-full flex items-center px-3.5 py-2.5 text-xs font-semibold rounded-lg transition {}", if active_page.get() == ActivePage::Dashboard { "bg-slate-950 text-white font-bold shadow-sm" } else { "text-slate-700 hover:bg-slate-100 hover:text-slate-900" })
                                            on:click=move |_| active_page.set(ActivePage::Dashboard)
                                        >
                                            <svg class="w-4 h-4 mr-2.5 opacity-80" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 5a1 1 0 011-1h14a1 1 0 011 1v2a1 1 0 01-1 1H5a1 1 0 01-1-1V5zM4 13a1 1 0 011-1h6a1 1 0 011 1v6a1 1 0 01-1 1H5a1 1 0 01-1-1v-6zM16 13a1 1 0 011-1h2a1 1 0 011 1v6a1 1 0 01-1 1h-2a1 1 0 01-1-1v-6z" />
                                            </svg>
                                            "Dashboard"
                                        </button>
                                    </div>

                                    // 2. EVENTS GROUP
                                    <div class="space-y-1">
                                        <div class="px-3 text-[10px] font-bold tracking-wider text-slate-400 uppercase">
                                            "Events"
                                        </div>
                                        <button
                                            class=move || format!("w-full flex items-center px-3.5 py-2 text-xs font-medium rounded-lg transition {}", if active_page.get() == ActivePage::Bindings { "bg-slate-950 text-white font-bold shadow-sm" } else { "text-slate-600 hover:bg-slate-100 hover:text-slate-900" })
                                            on:click=move |_| active_page.set(ActivePage::Bindings)
                                        >
                                            <svg class="w-4 h-4 mr-2.5 opacity-80" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 7h12m0 0l-4-4m4 4l-4 4m0 6H4m0 0l4 4m-4-4l4-4" />
                                            </svg>
                                            "Bindings"
                                        </button>
                                        <button
                                            class=move || format!("w-full flex items-center px-3.5 py-2 text-xs font-medium rounded-lg transition {}", if active_page.get() == ActivePage::Services { "bg-slate-950 text-white font-bold shadow-sm" } else { "text-slate-600 hover:bg-slate-100 hover:text-slate-900" })
                                            on:click=move |_| active_page.set(ActivePage::Services)
                                        >
                                            <svg class="w-4 h-4 mr-2.5 opacity-80" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10" />
                                            </svg>
                                            "Microsservices"
                                        </button>
                                        <button
                                            class=move || format!("w-full flex items-center px-3.5 py-2 text-xs font-medium rounded-lg transition {}", if active_page.get() == ActivePage::Queues { "bg-slate-950 text-white font-bold shadow-sm" } else { "text-slate-600 hover:bg-slate-100 hover:text-slate-900" })
                                            on:click=move |_| active_page.set(ActivePage::Queues)
                                        >
                                            <svg class="w-4 h-4 mr-2.5 opacity-80" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 12h16M4 18h16" />
                                            </svg>
                                            "Queues"
                                        </button>
                                        <button
                                            class=move || format!("w-full flex items-center px-3.5 py-2 text-xs font-medium rounded-lg transition {}", if active_page.get() == ActivePage::Connections { "bg-slate-950 text-white font-bold shadow-sm" } else { "text-slate-600 hover:bg-slate-100 hover:text-slate-900" })
                                            on:click=move |_| active_page.set(ActivePage::Connections)
                                        >
                                            <svg class="w-4 h-4 mr-2.5 opacity-80" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 7v10c0 2.21 3.582 4 8 4s8-1.79 8-4V7M4 7c0 2.21 3.582 4 8 4s8-1.79 8-4M4 7c0-2.21 3.582-4 8-4s8 1.79 8 4m0 5c0 2.21-3.582 4-8 4s-8-1.79-8-4" />
                                            </svg>
                                            "DB Pool"
                                        </button>
                                        <button
                                            class=move || format!("w-full flex items-center px-3.5 py-2 text-xs font-medium rounded-lg transition {}", if active_page.get() == ActivePage::Schedules { "bg-slate-950 text-white font-bold shadow-sm" } else { "text-slate-600 hover:bg-slate-100 hover:text-slate-900" })
                                            on:click=move |_| active_page.set(ActivePage::Schedules)
                                        >
                                            <svg class="w-4 h-4 mr-2.5 opacity-80" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
                                            </svg>
                                            "Schedules"
                                        </button>
                                    </div>

                                    // 3. FLOWS GROUP
                                    <div class="space-y-1">
                                        <div class="px-3 text-[10px] font-bold tracking-wider text-slate-400 uppercase">
                                            "Flows"
                                        </div>
                                        <button
                                            class=move || format!("w-full flex items-center px-3.5 py-2 text-xs font-medium rounded-lg transition {}", if active_page.get() == ActivePage::Logs { "bg-slate-950 text-white font-bold shadow-sm" } else { "text-slate-600 hover:bg-slate-100 hover:text-slate-900" })
                                            on:click=move |_| active_page.set(ActivePage::Logs)
                                        >
                                            <svg class="w-4 h-4 mr-2.5 opacity-80" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
                                            </svg>
                                            "Logs"
                                        </button>
                                        <button
                                            class=move || format!("w-full flex items-center px-3.5 py-2 text-xs font-medium rounded-lg transition {}", if active_page.get() == ActivePage::Flow { "bg-slate-950 text-white font-bold shadow-sm" } else { "text-slate-600 hover:bg-slate-100 hover:text-slate-900" })
                                            on:click=move |_| active_page.set(ActivePage::Flow)
                                        >
                                            <svg class="w-4 h-4 mr-2.5 opacity-80" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2zM7 7h10M7 11h10M7 15h10" />
                                            </svg>
                                            "Views"
                                        </button>
                                    </div>

                                    // 4. DEVELOPER GROUP
                                    <div class="space-y-1">
                                        <div class="px-3 text-[10px] font-bold tracking-wider text-slate-400 uppercase">
                                            "Developer"
                                        </div>
                                        <button
                                            class=move || format!("w-full flex items-center px-3.5 py-2 text-xs font-medium rounded-lg transition {}", if active_page.get() == ActivePage::ApiKeys { "bg-slate-950 text-white font-bold shadow-sm" } else { "text-slate-600 hover:bg-slate-100 hover:text-slate-900" })
                                            on:click=move |_| active_page.set(ActivePage::ApiKeys)
                                        >
                                            <svg class="w-4 h-4 mr-2.5 opacity-80" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 7a2 2 0 012 2m-2 4a5 5 0 01-10 0m0 0a5 5 0 0110 0m-5-5a2 2 0 11-4 0 2 2 0 014 0z" />
                                            </svg>
                                            "API Key"
                                        </button>
                                        <button
                                            class=move || format!("w-full flex items-center px-3.5 py-2 text-xs font-medium rounded-lg transition {}", if active_page.get() == ActivePage::Docs { "bg-slate-950 text-white font-bold shadow-sm" } else { "text-slate-600 hover:bg-slate-100 hover:text-slate-900" })
                                            on:click=move |_| active_page.set(ActivePage::Docs)
                                        >
                                            <svg class="w-4 h-4 mr-2.5 opacity-80" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.747 0 3.332.477 4.5 1.253v13C19.832 18.477 18.247 18 16.5 18c-1.746 0-3.332.477-4.5 1.253" />
                                            </svg>
                                            "Docs"
                                        </button>
                                        <button
                                            class=move || format!("w-full flex items-center px-3.5 py-2 text-xs font-medium rounded-lg transition {}", if active_page.get() == ActivePage::SdkDoc { "bg-slate-950 text-white font-bold shadow-sm" } else { "text-slate-600 hover:bg-slate-100 hover:text-slate-900" })
                                            on:click=move |_| active_page.set(ActivePage::SdkDoc)
                                        >
                                            <svg class="w-4 h-4 mr-2.5 opacity-80" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" />
                                            </svg>
                                            <div class="flex items-center justify-between flex-1">
                                                <span>"SDK Doc"</span>
                                                <span class="text-[9px] px-1.5 py-0.2 bg-indigo-100 text-indigo-700 font-bold rounded">"lib"</span>
                                            </div>
                                        </button>
                                    </div>

                                    // 5. BACKUP GROUP
                                    <div class="space-y-1">
                                        <div class="px-3 text-[10px] font-bold tracking-wider text-slate-400 uppercase">
                                            "Backup"
                                        </div>
                                        <label class="w-full flex items-center px-3.5 py-2 text-xs font-medium text-slate-600 hover:bg-slate-100 hover:text-slate-900 rounded-lg transition cursor-pointer gap-2.5">
                                            <svg class="w-4 h-4 opacity-80" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12" />
                                            </svg>
                                            <span>"Import"</span>
                                            <input
                                                type="file"
                                                accept=".json"
                                                class="hidden"
                                                on:change=move |ev| {
                                                    let file_input: web_sys::HtmlInputElement = ev.target().unwrap().unchecked_into();
                                                    if let Some(files) = file_input.files() {
                                                        if let Some(file) = files.get(0) {
                                                            let reader = web_sys::FileReader::new().unwrap();
                                                            let reader_c = reader.clone();
                                                            let onload = Closure::<dyn FnMut()>::new(move || {
                                                                let result = reader_c.result().unwrap();
                                                                let text = result.as_string().unwrap_or_default();
                                                                if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&text) {
                                                                    spawn_local(async move {
                                                                        match api::import_system_data(json_val).await {
                                                                            Ok(_) => {
                                                                                let _ = web_sys::window().unwrap().alert_with_message("Sistema restaurado com sucesso! Recarregando...");
                                                                                if let Some(w) = web_sys::window() {
                                                                                    let loc = w.location();
                                                                                    let _ = loc.reload();
                                                                                }
                                                                            }
                                                                            Err(e) => {
                                                                                let _ = web_sys::window().unwrap().alert_with_message(&format!("Erro ao importar: {}", e));
                                                                            }
                                                                        }
                                                                    });
                                                                } else {
                                                                    let _ = web_sys::window().unwrap().alert_with_message("Arquivo de backup JSON inválido.");
                                                                }
                                                            });
                                                            reader.set_onload(Some(onload.as_ref().unchecked_ref()));
                                                            let _ = reader.read_as_text(&file);
                                                            onload.forget();
                                                        }
                                                    }
                                                }
                                            />
                                        </label>
                                        <button
                                            class="w-full flex items-center px-3.5 py-2 text-xs font-medium text-slate-600 hover:bg-slate-100 hover:text-slate-900 rounded-lg transition text-left"
                                            on:click=move |_| do_export_backup()
                                        >
                                            <svg class="w-4 h-4 mr-2.5 opacity-80" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
                                            </svg>
                                            <span>"Export"</span>
                                        </button>
                                        <button
                                            class=move || format!("w-full flex items-center px-3.5 py-2 text-xs font-medium rounded-lg transition {}", if active_page.get() == ActivePage::Backup { "bg-slate-950 text-white font-bold shadow-sm" } else { "text-slate-600 hover:bg-slate-100 hover:text-slate-900" })
                                            on:click=move |_| active_page.set(ActivePage::Backup)
                                        >
                                            <svg class="w-4 h-4 mr-2.5 opacity-80" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 8h14M5 8a2 2 0 110-4h14a2 2 0 110 4M5 8v10a2 2 0 002 2h10a2 2 0 002-2V8m-9 4h4" />
                                            </svg>
                                            <span>"Backup Hub"</span>
                                        </button>
                                    </div>
                                </nav>

                                // 6. LOGOUT FOOTER
                                <div class="p-3 border-t border-slate-200">
                                    <button
                                        class="w-full py-2 bg-slate-100 hover:bg-red-50 text-slate-600 hover:text-red-700 text-xs font-semibold rounded-lg transition flex items-center justify-center gap-2 border border-slate-200 hover:border-red-200"
                                        on:click=move |_| {
                                            if let Some(window) = web_sys::window() {
                                                if let Ok(Some(storage)) = window.local_storage() {
                                                    let _ = storage.remove_item("auth_token");
                                                }
                                            }
                                            logged_in.set(false);
                                            active_page.set(ActivePage::Login);
                                        }
                                    >
                                        <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 16l4-4m0 0l-4-4m4 4H7m6 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h4a3 3 0 013 3v1" />
                                        </svg>
                                        "Logout"
                                    </button>
                                </div>
                            </aside>

                            // Page Content Area
                            <main class="flex-1 flex flex-col overflow-y-auto bg-slate-50 text-slate-800">
                                {move || match active_page.get() {
                                    ActivePage::Dashboard => view! { <DashboardView /> }.into_any(),
                                    ActivePage::SystemHealth => view! { <SystemHealthView /> }.into_any(),
                                    ActivePage::Services => view! { <ServicesView /> }.into_any(),
                                    ActivePage::Connections => view! { <ConnectionsView /> }.into_any(),
                                    ActivePage::Queues => view! { <QueuesView /> }.into_any(),
                                    ActivePage::Bindings => view! { <BindingsView /> }.into_any(),
                                    ActivePage::Logs => view! { <LogsView /> }.into_any(),
                                    ActivePage::Flow => view! { <FlowView /> }.into_any(),
                                    ActivePage::Docs => view! { <DocsView /> }.into_any(),
                                    ActivePage::SdkDoc => view! { <SdkDocView /> }.into_any(),
                                    ActivePage::ApiKeys => view! { <ApiKeysView /> }.into_any(),
                                    ActivePage::Schedules => view! { <SchedulesView /> }.into_any(),
                                    ActivePage::Backup => view! { <BackupView /> }.into_any(),
                                    ActivePage::Login => view! { <div>"Please login"</div> }.into_any(),
                                }}
                            </main>
                        </div>
                    }.into_any()
                }
            }}
        </div>
    }
}

// =============================================================================
// Dashboard (Active Microservices, Queues, Schedules & Last Executions)
// =============================================================================
#[derive(Clone, Copy, PartialEq, Debug)]
enum DashboardTab {
    Ready,
    Schedules,
    AllActive,
}

#[component]
fn DashboardView() -> impl IntoView {
    let services = RwSignal::new(Vec::<MicroserviceDTO>::new());
    let bindings = RwSignal::new(Vec::<BindingDTO>::new());
    let queues = RwSignal::new(Vec::<QueueDTO>::new());
    let schedules = RwSignal::new(Vec::<ScheduledJobDTO>::new());
    let recent_logs = RwSignal::new(Vec::<ExecutionLogDTO>::new());
    let is_loading = RwSignal::new(true);
    let selected_tab = RwSignal::new(DashboardTab::Ready);
    let active_page_ctx = use_context::<RwSignal<ActivePage>>();

    let reload_data = move || {
        is_loading.set(true);
        spawn_local(async move {
            if let Ok(svc) = api::get_services().await {
                let active_svc: Vec<MicroserviceDTO> = svc.into_iter().filter(|s| s.is_active).collect();
                services.set(active_svc);
            }
            if let Ok(b) = api::get_bindings().await {
                bindings.set(b);
            }
            if let Ok(q) = api::get_queues().await {
                queues.set(q);
            }
            if let Ok(sch) = api::get_schedules().await {
                schedules.set(sch);
            }
            if let Ok(log_res) = api::search_logs(LogFilterQuery {
                microservice_id: None,
                queue_id: None,
                status: None,
                tags: None,
                start_date: None,
                end_date: None,
                min_duration_ms: None,
                max_duration_ms: None,
                search_term: None,
                page: 1,
                limit: 100,
            }).await {
                recent_logs.set(log_res.logs);
            }
            is_loading.set(false);
        });
    };

    Effect::new(move |_| {
        reload_data();
    });

    view! {
        <div class="p-8 space-y-6 max-w-7xl mx-auto">
            // Header & Actions
            <div class="flex flex-col md:flex-row md:items-center md:justify-between gap-4">
                <div>
                    <h1 class="text-2xl font-extrabold text-slate-900 tracking-tight">"Active Microservices Dashboard"</h1>
                    <p class="text-sm text-slate-500 mt-1">"Monitor active microservices, their attached queues/streams, scheduled jobs, and latest execution status."</p>
                </div>
                <button
                    class="self-start md:self-auto inline-flex items-center gap-2 px-4 py-2 bg-white border border-slate-200 hover:bg-slate-50 text-slate-700 text-sm font-semibold rounded-lg shadow-sm transition active:scale-95"
                    on:click=move |_| reload_data()
                >
                    <svg class=move || format!("w-4 h-4 text-slate-500 {}", if is_loading.get() { "animate-spin" } else { "" }) fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
                    </svg>
                    "Refresh"
                </button>
            </div>

            // Summary Metric Cards
            <div class="grid grid-cols-2 sm:grid-cols-4 gap-4">
                <div class="bg-white border border-slate-200 rounded-xl p-4 shadow-sm flex flex-col justify-between">
                    <span class="text-xs font-semibold text-slate-500 uppercase tracking-wider">"Active Services"</span>
                    <div class="flex items-baseline justify-between mt-2">
                        <span class="text-2xl font-black text-slate-900">{move || services.get().len()}</span>
                        <span class="p-1.5 rounded-lg bg-emerald-50 text-emerald-600 text-xs font-bold">"LIVE"</span>
                    </div>
                </div>
                <div class="bg-white border border-slate-200 rounded-xl p-4 shadow-sm flex flex-col justify-between">
                    <span class="text-xs font-semibold text-slate-500 uppercase tracking-wider">"Active Queues"</span>
                    <div class="flex items-baseline justify-between mt-2">
                        <span class="text-2xl font-black text-slate-900">{move || queues.get().iter().filter(|q| q.is_active).count()}</span>
                        <span class="p-1.5 rounded-lg bg-amber-50 text-amber-600 text-xs font-bold">"REDIS"</span>
                    </div>
                </div>
                <div class="bg-white border border-slate-200 rounded-xl p-4 shadow-sm flex flex-col justify-between">
                    <span class="text-xs font-semibold text-slate-500 uppercase tracking-wider">"Active Bindings"</span>
                    <div class="flex items-baseline justify-between mt-2">
                        <span class="text-2xl font-black text-slate-900">{move || bindings.get().iter().filter(|b| b.is_active).count()}</span>
                        <span class="p-1.5 rounded-lg bg-indigo-50 text-indigo-600 text-xs font-bold">"EVENT"</span>
                    </div>
                </div>
                <div class="bg-white border border-slate-200 rounded-xl p-4 shadow-sm flex flex-col justify-between">
                    <span class="text-xs font-semibold text-slate-500 uppercase tracking-wider">"Scheduled Jobs"</span>
                    <div class="flex items-baseline justify-between mt-2">
                        <span class="text-2xl font-black text-slate-900">{move || schedules.get().len()}</span>
                        <span class="p-1.5 rounded-lg bg-purple-50 text-purple-600 text-xs font-bold">"CRON"</span>
                    </div>
                </div>
            </div>

            // Navigation Tabs
            <div class="border-b border-slate-200 flex space-x-2">
                <button
                    class=move || format!(
                        "px-4 py-2.5 text-sm font-semibold border-b-2 transition flex items-center gap-2 {}",
                        if selected_tab.get() == DashboardTab::Ready {
                            "border-black text-slate-950 font-bold"
                        } else {
                            "border-transparent text-slate-500 hover:text-slate-800 hover:border-slate-300"
                        }
                    )
                    on:click=move |_| selected_tab.set(DashboardTab::Ready)
                >
                    <span>"Ready (Aguardando Queue)"</span>
                    {move || {
                        let count = services.get().iter().filter(|s| {
                            let ms_id = s.id.clone().unwrap_or_default();
                            bindings.get().iter().any(|b| b.is_active && b.microservice_id == ms_id)
                        }).count();
                        view! {
                            <span class="px-2 py-0.5 rounded-full text-xs font-bold bg-amber-100 text-amber-800">
                                {count}
                            </span>
                        }
                    }}
                </button>
                <button
                    class=move || format!(
                        "px-4 py-2.5 text-sm font-semibold border-b-2 transition flex items-center gap-2 {}",
                        if selected_tab.get() == DashboardTab::Schedules {
                            "border-black text-slate-950 font-bold"
                        } else {
                            "border-transparent text-slate-500 hover:text-slate-800 hover:border-slate-300"
                        }
                    )
                    on:click=move |_| selected_tab.set(DashboardTab::Schedules)
                >
                    <span>"Schedules (Agendados)"</span>
                    {move || {
                        let count = services.get().iter().filter(|s| {
                            let ms_id = s.id.clone().unwrap_or_default();
                            schedules.get().iter().any(|sch| sch.microservice_id == ms_id)
                        }).count();
                        view! {
                            <span class="px-2 py-0.5 rounded-full text-xs font-bold bg-purple-100 text-purple-800">
                                {count}
                            </span>
                        }
                    }}
                </button>
                <button
                    class=move || format!(
                        "px-4 py-2.5 text-sm font-semibold border-b-2 transition flex items-center gap-2 {}",
                        if selected_tab.get() == DashboardTab::AllActive {
                            "border-black text-slate-950 font-bold"
                        } else {
                            "border-transparent text-slate-500 hover:text-slate-800 hover:border-slate-300"
                        }
                    )
                    on:click=move |_| selected_tab.set(DashboardTab::AllActive)
                >
                    <span>"Todos Ativos"</span>
                    {move || {
                        let count = services.get().len();
                        view! {
                            <span class="px-2 py-0.5 rounded-full text-xs font-bold bg-slate-100 text-slate-700">
                                {count}
                            </span>
                        }
                    }}
                </button>
            </div>

            // Microservice Cards Grid
            <div>
                {move || {
                    let cur_tab = selected_tab.get();
                    let all_active = services.get();
                    let all_bindings = bindings.get();
                    let all_queues = queues.get();
                    let all_schedules = schedules.get();
                    let all_logs = recent_logs.get();

                    let filtered_services: Vec<MicroserviceDTO> = all_active.into_iter().filter(|s| {
                        let ms_id = s.id.clone().unwrap_or_default();
                        match cur_tab {
                            DashboardTab::Ready => {
                                all_bindings.iter().any(|b| b.is_active && b.microservice_id == ms_id)
                            },
                            DashboardTab::Schedules => {
                                all_schedules.iter().any(|sch| sch.microservice_id == ms_id)
                            },
                            DashboardTab::AllActive => true,
                        }
                    }).collect();

                    if filtered_services.is_empty() {
                        view! {
                            <div class="text-center py-16 bg-white border border-slate-200 border-dashed rounded-xl p-8">
                                <div class="w-12 h-12 rounded-full bg-slate-100 text-slate-400 flex items-center justify-center mx-auto mb-3">
                                    <svg class="w-6 h-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10" />
                                    </svg>
                                </div>
                                <h3 class="text-base font-bold text-slate-800">"Nenhum microsserviço ativo encontrado nesta aba"</h3>
                                <p class="text-xs text-slate-500 mt-1 max-w-sm mx-auto">
                                    {match cur_tab {
                                        DashboardTab::Ready => "Não há microsserviços ativos vinculados a filas Redis ativas no momento.",
                                        DashboardTab::Schedules => "Não há agendamentos (cron ou data futura) configurados para microsserviços ativos.",
                                        DashboardTab::AllActive => "Nenhum microsserviço está marcado como ativo no sistema.",
                                    }}
                                </p>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
                                {filtered_services.into_iter().map(|ms| {
                                    let ms_id = ms.id.clone().unwrap_or_default();
                                    
                                    // Attached queues
                                    let ms_bound_queues: Vec<String> = all_bindings.iter()
                                        .filter(|b| b.is_active && b.microservice_id == ms_id)
                                        .map(|b| {
                                            if let Some(q) = &b.queue {
                                                q.stream_key.clone()
                                            } else {
                                                all_queues.iter()
                                                    .find(|q| q.id.as_ref() == Some(&b.queue_id))
                                                    .map(|q| q.stream_key.clone())
                                                    .unwrap_or_else(|| format!("Queue #{}", b.queue_id))
                                            }
                                        })
                                        .collect();

                                    // Schedules
                                    let ms_scheds: Vec<ScheduledJobDTO> = all_schedules.iter()
                                        .filter(|sch| sch.microservice_id == ms_id)
                                        .cloned()
                                        .collect();

                                    // Last execution log
                                    let last_log = all_logs.iter()
                                        .find(|l| l.microservice_id == ms_id)
                                        .cloned();

                                    let active_ver_id = ms.active_version_id.clone();
                                    let active_ver_tag = ms.active_version_tag.clone().unwrap_or_else(|| "N/A".to_string());
                                    let desc = ms.description.clone().unwrap_or_default();
                                    let tags = ms.tags.clone();
                                    let name = ms.name.clone();

                                    view! {
                                        <div class="bg-white border border-slate-200 hover:border-slate-300 rounded-xl p-6 shadow-sm hover:shadow-md transition-all flex flex-col justify-between space-y-4">
                                            // Card Header
                                            <div>
                                                <div class="flex items-start justify-between gap-2">
                                                    <div class="flex items-center gap-2">
                                                        <span class="w-8 h-8 rounded-lg bg-orange-50 text-orange-600 border border-orange-200 flex items-center justify-center font-bold text-xs">
                                                            "🦀"
                                                        </span>
                                                        <div>
                                                            <h3 class="font-bold text-slate-900 text-base leading-tight">{name}</h3>
                                                            <span class="text-[11px] text-slate-400 font-mono">"ID: #" {ms_id.clone()}</span>
                                                        </div>
                                                    </div>
                                                    <span class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-bold bg-emerald-50 text-emerald-700 border border-emerald-200">
                                                        <span class="w-1.5 h-1.5 rounded-full bg-emerald-500 animate-pulse"></span>
                                                        "ACTIVE"
                                                    </span>
                                                </div>

                                                // Description & Tags
                                                {if !desc.is_empty() {
                                                    view! {
                                                        <p class="text-xs text-slate-500 mt-2 line-clamp-2">{desc}</p>
                                                    }.into_any()
                                                } else {
                                                    view! { <div></div> }.into_any()
                                                }}

                                                {if !tags.is_empty() {
                                                    view! {
                                                        <div class="flex flex-wrap gap-1 mt-2.5">
                                                            {tags.into_iter().map(|tag| view! {
                                                                <span class="px-2 py-0.5 rounded text-[10px] font-medium bg-slate-100 text-slate-600 border border-slate-200">
                                                                    "#" {tag}
                                                                </span>
                                                            }).collect::<Vec<_>>()}
                                                        </div>
                                                    }.into_any()
                                                } else {
                                                    view! { <div></div> }.into_any()
                                                }}
                                            </div>

                                            // Details: Version, Queues, Schedules, Last Execution
                                            <div class="space-y-3 pt-3 border-t border-slate-100 text-xs">
                                                // Version & Container Status
                                                <div class="flex items-center justify-between">
                                                    <span class="text-slate-500 font-medium">"Active Version:"</span>
                                                    <div class="flex items-center gap-2">
                                                        <span class="px-2 py-0.5 rounded text-[11px] font-mono font-semibold bg-indigo-50 text-indigo-700 border border-indigo-200">
                                                            {active_ver_tag}
                                                        </span>
                                                        {if let Some(v_id) = active_ver_id {
                                                            view! {
                                                                <VersionStatusBadge version_id=v_id />
                                                            }.into_any()
                                                        } else {
                                                            view! { <div></div> }.into_any()
                                                        }}
                                                    </div>
                                                </div>

                                                // Queues in Use
                                                <div>
                                                    <div class="text-slate-500 font-medium mb-1 flex items-center gap-1">
                                                        <svg class="w-3.5 h-3.5 text-amber-500" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
                                                        </svg>
                                                        <span>"Queues (Streams) em Escuta:"</span>
                                                    </div>
                                                    {if !ms_bound_queues.is_empty() {
                                                        view! {
                                                            <div class="flex flex-wrap gap-1.5">
                                                                {ms_bound_queues.into_iter().map(|q_name| view! {
                                                                    <span class="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[11px] font-mono font-medium bg-amber-50 text-amber-800 border border-amber-200">
                                                                        <span class="w-1.5 h-1.5 rounded-full bg-amber-500"></span>
                                                                        {q_name}
                                                                    </span>
                                                                }).collect::<Vec<_>>()}
                                                            </div>
                                                        }.into_any()
                                                    } else {
                                                        view! {
                                                            <span class="text-slate-400 italic text-[11px]">"Nenhuma fila vinculada (chamada direta)"</span>
                                                        }.into_any()
                                                    }}
                                                </div>

                                                // Schedules (if any)
                                                {if !ms_scheds.is_empty() {
                                                    view! {
                                                        <div>
                                                            <div class="text-slate-500 font-medium mb-1 flex items-center gap-1">
                                                                <svg class="w-3.5 h-3.5 text-purple-500" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
                                                                </svg>
                                                                <span>"Agendamentos:"</span>
                                                            </div>
                                                            <div class="flex flex-wrap gap-1.5">
                                                                {ms_scheds.into_iter().map(|sch| {
                                                                    let cron_str = sch.cron_expression.clone().unwrap_or_else(|| "One-time".to_string());
                                                                    view! {
                                                                        <span class="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[11px] font-mono font-medium bg-purple-50 text-purple-800 border border-purple-200">
                                                                            <span>"⏱️"</span>
                                                                            {cron_str}
                                                                        </span>
                                                                    }
                                                                }).collect::<Vec<_>>()}
                                                            </div>
                                                        </div>
                                                    }.into_any()
                                                } else {
                                                    view! { <div></div> }.into_any()
                                                }}

                                                // Last Execution Info
                                                <div class="pt-2 border-t border-slate-100">
                                                    <div class="text-slate-500 font-medium mb-1 flex items-center justify-between">
                                                        <span>"Última Execução:"</span>
                                                        {if let Some(log) = &last_log {
                                                            let rel_time = format_relative_time(log.created_at);
                                                            view! {
                                                                <span class="text-[10px] text-slate-400 font-medium">{rel_time}</span>
                                                            }.into_any()
                                                        } else {
                                                            view! { <div></div> }.into_any()
                                                        }}
                                                    </div>
                                                    {if let Some(log) = &last_log {
                                                        let st = log.status.clone();
                                                        let exec_ms = log.execution_time_ms;
                                                        let err_msg = log.error_message.clone();
                                                        let is_success = st == "success";
                                                        let is_timeout = st == "timeout";
                                                        let badge_class = if is_success {
                                                            "px-2 py-0.5 rounded text-[10px] font-bold uppercase tracking-wider border bg-emerald-50 text-emerald-700 border-emerald-200"
                                                        } else if is_timeout {
                                                            "px-2 py-0.5 rounded text-[10px] font-bold uppercase tracking-wider border bg-amber-50 text-amber-700 border-amber-200"
                                                        } else {
                                                            "px-2 py-0.5 rounded text-[10px] font-bold uppercase tracking-wider border bg-red-50 text-red-700 border-red-200"
                                                        };
                                                        view! {
                                                            <div class="space-y-1">
                                                                <div class="flex items-center justify-between">
                                                                    <span class=badge_class>
                                                                        {st}
                                                                    </span>
                                                                    <span class="text-[11px] font-mono text-slate-600 font-medium">
                                                                        "⚡ " {exec_ms} "ms"
                                                                    </span>
                                                                </div>
                                                                {if let Some(err) = err_msg {
                                                                    view! {
                                                                        <div class="mt-1 p-1.5 rounded bg-red-50 text-red-700 font-mono text-[10px] line-clamp-1 border border-red-100">
                                                                            {err}
                                                                        </div>
                                                                    }.into_any()
                                                                } else {
                                                                    view! { <div></div> }.into_any()
                                                                }}
                                                            </div>
                                                        }.into_any()
                                                    } else {
                                                        view! {
                                                            <span class="text-slate-400 italic text-[11px]">"Nenhuma execução registrada"</span>
                                                        }.into_any()
                                                    }}
                                                </div>
                                            </div>

                                            // Card Footer Actions
                                            <div class="pt-3 border-t border-slate-100 flex items-center justify-between gap-2">
                                                <button
                                                    class="flex-1 py-1.5 px-3 rounded-lg bg-slate-900 hover:bg-black text-white text-xs font-semibold shadow-sm transition text-center"
                                                    on:click=move |_| {
                                                        if let Some(ctx) = active_page_ctx {
                                                            ctx.set(ActivePage::Services);
                                                        }
                                                    }
                                                >
                                                    "Gerenciar Código"
                                                </button>
                                                <button
                                                    class="py-1.5 px-3 rounded-lg bg-slate-100 hover:bg-slate-200 text-slate-700 text-xs font-medium transition text-center"
                                                    on:click=move |_| {
                                                        if let Some(ctx) = active_page_ctx {
                                                            ctx.set(ActivePage::Logs);
                                                        }
                                                    }
                                                >
                                                    "Ver Logs"
                                                </button>
                                            </div>
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        }.into_any()
                    }
                }}
            </div>
        </div>
    }
}

// =============================================================================
// System Health & Live Monitoring View
// =============================================================================
#[component]
fn SystemHealthView() -> impl IntoView {
    let health_data = RwSignal::new(None::<SystemHealthResponse>);

    let reload = move || {
        spawn_local(async move {
            if let Ok(data) = api::get_system_health().await {
                health_data.set(Some(data));
            }
        });
    };

    Effect::new(move |_| {
        reload();
        spawn_local(async move {
            loop {
                TimeoutFuture::new(2000).await;
                if let Ok(data) = api::get_system_health().await {
                    health_data.set(Some(data));
                }
            }
        });
    });

    view! {
        <div class="p-8 space-y-6">
            <h1 class="text-2xl font-bold text-slate-900">"System Health & Live Monitoring"</h1>
            
            {move || health_data.get().map(|hd| {
                let mem_pct = if hd.host.memory_total_kb > 0 {
                    (hd.host.memory_used_kb as f32 / hd.host.memory_total_kb as f32) * 100.0
                } else {
                    0.0
                };

                view! {
                    <div class="grid grid-cols-1 md:grid-cols-3 gap-6">
                        // CPU Card
                        <div class="bg-white border border-slate-200 rounded-xl p-6 shadow-sm">
                            <span class="text-xs font-semibold text-slate-500 uppercase tracking-wider block mb-2">"CPU Usage (Total)"</span>
                            <div class="flex items-baseline space-x-2">
                                <span class="text-3xl font-extrabold text-slate-900">{format!("{:.1}%", hd.host.cpu_usage_total)}</span>
                            </div>
                            <div class="mt-4 h-2 bg-slate-100 rounded-full overflow-hidden">
                                <div class="bg-indigo-600 h-full" style=format!("width: {}%", hd.host.cpu_usage_total)></div>
                            </div>
                        </div>

                        // RAM Card
                        <div class="bg-white border border-slate-200 rounded-xl p-6 shadow-sm">
                            <span class="text-xs font-semibold text-slate-500 uppercase tracking-wider block mb-2">"Memory (RAM)"</span>
                            <div class="flex items-baseline space-x-2">
                                <span class="text-3xl font-extrabold text-slate-900">{format!("{:.1}%", mem_pct)}</span>
                                <span class="text-xs text-slate-500">{format!("({} MB / {} MB)", hd.host.memory_used_kb / 1024, hd.host.memory_total_kb / 1024)}</span>
                            </div>
                            <div class="mt-4 h-2 bg-slate-100 rounded-full overflow-hidden">
                                <div class="bg-emerald-600 h-full" style=format!("width: {}%", mem_pct)></div>
                            </div>
                        </div>

                        // Uptime Card
                        <div class="bg-white border border-slate-200 rounded-xl p-6 shadow-sm">
                            <span class="text-xs font-semibold text-slate-500 uppercase tracking-wider block mb-2">"System Uptime"</span>
                            <span class="text-3xl font-extrabold text-slate-900">{format!("{}s", hd.host.uptime_seconds)}</span>
                            <span class="text-xs text-slate-500 block mt-2">{format!("Load Average: {:.2}, {:.2}, {:.2}", hd.host.load_average.0, hd.host.load_average.1, hd.host.load_average.2)}</span>
                        </div>
                    </div>

                    // Active Containers Panel
                    <div class="bg-white border border-slate-200 rounded-xl overflow-hidden mt-8 shadow-sm">
                        <div class="px-6 py-4 border-b border-slate-200 flex justify-between items-center bg-slate-50">
                            <h3 class="font-bold text-slate-800">"Active Docker Containers"</h3>
                            <span class="px-2.5 py-0.5 rounded-full text-xs font-medium bg-slate-200 text-slate-700">
                                {hd.containers.len()} " Containers"
                            </span>
                        </div>
                        <table class="w-full text-left">
                            <thead class="bg-slate-50 border-b border-slate-200 text-xs font-semibold text-slate-500 uppercase">
                                <tr>
                                    <th class="px-6 py-3">"ID / Name"</th>
                                    <th class="px-6 py-3">"Status"</th>
                                    <th class="px-6 py-3">"CPU Usage"</th>
                                    <th class="px-6 py-3">"Memory"</th>
                                </tr>
                            </thead>
                            <tbody class="divide-y divide-slate-200 text-sm">
                                {hd.containers.into_iter().map(|container| {
                                    let status = container.status.clone();
                                    view! {
                                        <tr class="hover:bg-slate-50/50">
                                            <td class="px-6 py-4 font-mono text-xs">
                                                <span class="text-slate-900 font-medium block">{container.name}</span>
                                                <span class="text-slate-400">{container.id.chars().take(12).collect::<String>()}</span>
                                            </td>
                                            <td class="px-6 py-4">
                                                <span class=format!("px-2 py-0.5 rounded-full text-xs font-semibold {}", if status == "running" { "bg-emerald-50 text-emerald-700 border border-emerald-200" } else { "bg-slate-100 text-slate-500 border border-slate-200" })>
                                                    {container.status}
                                                </span>
                                            </td>
                                            <td class="px-6 py-4 font-mono">{format!("{:.2}%", container.cpu_usage_percent)}</td>
                                            <td class="px-6 py-4 font-mono text-xs">{format!("{} MB / {} MB", container.memory_usage_bytes / 1024 / 1024, container.memory_limit_bytes / 1024 / 1024)}</td>
                                        </tr>
                                    }
                                }).collect::<Vec<_>>()}
                            </tbody>
                        </table>
                    </div>
                }
            })}
        </div>
    }
}

// =============================================================================
// Microservices Management View
fn calculate_next_tag(latest: &str) -> String {
    let parts: Vec<&str> = latest.split('.').collect();
    if parts.len() == 3 {
        let last_part = parts[2];
        let digit_str: String = last_part.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(num) = digit_str.parse::<i32>() {
            let next_num = num + 1;
            let rest = &last_part[digit_str.len()..];
            return format!("{}.{}.{}{}", parts[0], parts[1], next_num, rest);
        }
    }
    "1.0.0".to_string()
}

#[component]
fn VersionStatusBadge(version_id: String) -> impl IntoView {
    let container_status = RwSignal::new("loading".to_string());
    
    let ver_id_c = version_id.clone();
    Effect::new(move |_| {
        let ver_id = ver_id_c.clone();
        spawn_local(async move {
            if let Ok(st) = api::get_version_container_status(&ver_id).await {
                container_status.set(st);
            } else {
                container_status.set("error".to_string());
            }
        });
    });

    view! {
        <div class="flex items-center gap-1.5 mt-0.5">
            {move || {
                let status = container_status.get();
                if status == "loading" {
                    view! {
                        <div class="flex items-center gap-1 text-slate-400 font-medium text-[10px]">
                            <svg class="animate-spin h-3.5 w-3.5 text-slate-400" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                            </svg>
                            <span>"Checking..."</span>
                        </div>
                    }.into_any()
                } else if status == "running" {
                    view! {
                        <span class="px-1.5 py-0.5 rounded text-[10px] font-bold bg-emerald-50 text-emerald-700 border border-emerald-200 animate-pulse">
                            "RUNNING"
                        </span>
                    }.into_any()
                } else if status == "stopped" {
                    view! {
                        <span class="px-1.5 py-0.5 rounded text-[10px] font-bold bg-slate-100 text-slate-600 border border-slate-200">
                            "STOPPED"
                        </span>
                    }.into_any()
                } else if status == "not_compiled" {
                    view! {
                        <span class="px-1.5 py-0.5 rounded text-[10px] font-bold bg-amber-50 text-amber-700 border border-amber-200">
                            "NOT COMPILED"
                        </span>
                    }.into_any()
                } else {
                    view! {
                        <span class="px-1.5 py-0.5 rounded text-[10px] font-bold bg-red-50 text-red-700 border border-red-200">
                            "ERROR"
                        </span>
                    }.into_any()
                }
            }}
        </div>
    }
}

#[component]
fn ServicesView() -> impl IntoView {
    let services = RwSignal::new(Vec::<MicroserviceDTO>::new());
    let ms_page = RwSignal::new(1);
    let show_only_active = RwSignal::new(true);
    let new_is_active = RwSignal::new(true);
    
    let new_name = RwSignal::new(String::new());
    let new_desc = RwSignal::new(String::new());
    let tags_list = RwSignal::new(Vec::<String>::new());
    let tag_input = RwSignal::new(String::new());

    let success_action = RwSignal::new("end".to_string());
    let success_config = RwSignal::new(String::new());
    let error_action = RwSignal::new("end".to_string());
    let error_config = RwSignal::new(String::new());

    let editing_service_id = RwSignal::new(None::<String>);
    let queues = RwSignal::new(Vec::<QueueDTO>::new());

    let show_deploy_modal = RwSignal::new(false);
    let envs = RwSignal::new(Vec::<MicroserviceEnvDTO>::new());
    let new_env_name = RwSignal::new(String::new());
    let new_env_config = RwSignal::new(String::new());
    let new_env_is_default = RwSignal::new(false);

    let success_ke_key = RwSignal::new(String::new());
    let success_ke_operator = RwSignal::new("==".to_string());
    let success_ke_value = RwSignal::new(String::new());
    let success_ke_dest = RwSignal::new(String::new());

    let error_ke_key = RwSignal::new(String::new());
    let error_ke_operator = RwSignal::new("==".to_string());
    let error_ke_value = RwSignal::new(String::new());
    let error_ke_dest = RwSignal::new(String::new());

    let selected_service = RwSignal::new(None::<MicroserviceDTO>);
    let ide_files = RwSignal::new(vec![
        ("src/main.rs".to_string(), "fn main() {\n    println!(\"Hello from DooPack!\");\n}".to_string()),
        ("Cargo.toml".to_string(), "[package]\nname = \"service\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nserde = { version = \"1.0\", features = [\"derive\"] }\nserde_json = \"1.0\"\ntokio = { version = \"1.0\", features = [\"full\"] }\nrust-sdk = { path = \"./rust-sdk\" }\n".to_string())
    ]);
    let ide_active_file = RwSignal::new("src/main.rs".to_string());
    let ide_new_file_name = RwSignal::new(String::new());
    let ide_show_new_file_input = RwSignal::new(false);
    let version_tag = RwSignal::new(String::new());
    let build_status = RwSignal::new(None::<String>);
    let build_logs_stream = RwSignal::new(String::new());
    let build_in_progress = RwSignal::new(false);
    let testing_version_id = RwSignal::new(None::<String>);
    let test_payload_input = RwSignal::new("{\n  \"id\": 123\n}".to_string());
    let test_result_output = RwSignal::new(None::<String>);
    let test_in_progress = RwSignal::new(false);
    let source_type = RwSignal::new("textarea".to_string());
    let version_code = RwSignal::new(String::new());
    let service_versions = RwSignal::new(Vec::<MicroserviceVersionDTO>::new());
    let version_page = RwSignal::new(1);
    let deploy_modal_tab = RwSignal::new("deploy".to_string());

    if let Some(window) = web_sys::window() {
        use wasm_bindgen::JsCast;
        let show_deploy_modal_c = show_deploy_modal.clone();
        let testing_version_id_c = testing_version_id.clone();
        let editing_service_id_c = editing_service_id.clone();
        let cb = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            show_deploy_modal_c.set(false);
            testing_version_id_c.set(None);
            editing_service_id_c.set(None);
        });
        let _ = window.add_event_listener_with_callback("close_all_modals", cb.as_ref().unchecked_ref());
        cb.forget();
    }

    let load_versions = move |s_id: String| {
        version_page.set(1);
        spawn_local(async move {
            if let Ok(list) = api::get_versions(&s_id).await {
                let next_tag = if list.is_empty() {
                    "1.0.0".to_string()
                } else {
                    calculate_next_tag(&list[0].version_tag)
                };
                version_tag.set(next_tag);
                service_versions.set(list.clone());
                
                if !list.is_empty() {
                    let latest_code = &list[0].source_code;
                    if latest_code.trim().starts_with('{') {
                        if let Ok(files_map) = serde_json::from_str::<std::collections::HashMap<String, String>>(latest_code) {
                            let mut sorted_files: Vec<(String, String)> = files_map.into_iter().collect();
                            sorted_files.sort_by(|a, b| a.0.cmp(&b.0));
                            ide_files.set(sorted_files);
                        }
                    } else {
                        ide_files.set(vec![
                            ("src/main.rs".to_string(), latest_code.clone()),
                            ("Cargo.toml".to_string(), "[package]\nname = \"service\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nserde = { version = \"1.0\", features = [\"derive\"] }\nserde_json = \"1.0\"\ntokio = { version = \"1.0\", features = [\"full\"] }\nrust-sdk = { path = \"./rust-sdk\" }\n".to_string())
                        ]);
                    }
                } else {
                    ide_files.set(vec![
                        ("src/main.rs".to_string(), "fn main() {\n    println!(\"Hello from DooPack!\");\n}".to_string()),
                        ("Cargo.toml".to_string(), "[package]\nname = \"service\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nserde = { version = \"1.0\", features = [\"derive\"] }\nserde_json = \"1.0\"\ntokio = { version = \"1.0\", features = [\"full\"] }\nrust-sdk = { path = \"./rust-sdk\" }\n".to_string())
                    ]);
                }
                ide_active_file.set("src/main.rs".to_string());
            }
        });
    };

    let reload = move || {
        spawn_local(async move {
            if let Ok(q_list) = api::get_queues().await {
                queues.set(q_list);
            }
            if let Ok(list) = api::get_services().await {
                if let Some(ref sel) = selected_service.get_untracked() {
                    if let Some(updated) = list.iter().find(|x| x.id == sel.id) {
                        selected_service.set(Some(updated.clone()));
                    }
                }
                services.set(list);
            }
        });
    };

    Effect::new(move |_| {
        reload();
    });

    view! {
        <div class="p-8 space-y-6">
            <div class="flex justify-between items-center">
                <h1 class="text-2xl font-bold text-slate-900">"Microservices Directory"</h1>
                <label class="inline-flex items-center gap-2 text-sm font-semibold text-slate-600 bg-white px-3 py-1.5 border border-slate-200 rounded-lg shadow-sm cursor-pointer hover:bg-slate-50 transition">
                    <input
                        type="checkbox"
                        class="rounded border-slate-300 text-indigo-600 focus:ring-indigo-500 h-4 w-4"
                        prop:checked=show_only_active
                        on:change=move |ev| show_only_active.set(event_target_checked(&ev))
                    />
                    "Show Active Only"
                </label>
            </div>

            <div class="grid grid-cols-1 lg:grid-cols-3 gap-8">
                // Left: List & Config
                <div class="lg:col-span-2 space-y-6">
                    <div class="bg-white border border-slate-200 rounded-xl overflow-hidden shadow-sm">
                        <table class="w-full text-left">
                            <thead class="bg-slate-50 border-b border-slate-200 text-xs font-semibold text-slate-500 uppercase">
                                <tr>
                                    <th class="px-6 py-3">"ID"</th>
                                    <th class="px-6 py-3">"Name / Description"</th>
                                    <th class="px-6 py-3">"Tags"</th>
                                    <th class="px-6 py-3">"Actions"</th>
                                </tr>
                            </thead>
                            <tbody class="divide-y divide-slate-200 text-sm">
                                {move || {
                                    let raw_list = services.get();
                                    let list = if show_only_active.get() {
                                        raw_list.into_iter().filter(|s| s.is_active).collect::<Vec<_>>()
                                    } else {
                                        raw_list
                                    };
                                    let total_len = list.len();
                                    let page = ms_page.get();
                                    let start = (page - 1) * 5;
                                    let end = std::cmp::min(page * 5, total_len);

                                    if total_len == 0 {
                                        return view! {
                                            <tr>
                                                <td colspan="4" class="px-6 py-8 text-center text-slate-400">
                                                    "No microservices found"
                                                </td>
                                            </tr>
                                        }.into_any();
                                    }

                                    let page_slice = list[start..end].to_vec();
                                    page_slice.into_iter().map(|service| {
                                        let s_clone = service.clone();
                                        let s_clone_edit = s_clone.clone();
                                        let s_id = service.id.clone().unwrap_or_default();
                                        let s_id_deploy = s_id.clone();
                                        let s_id_delete = s_id.clone();
                                        view! {
                                            <tr class="hover:bg-slate-50/50">
                                                <td class="px-6 py-4 font-mono text-xs">
                                                    <span class="px-2 py-1 bg-slate-100 text-slate-800 rounded font-bold border border-slate-200">
                                                        "#" {s_id.clone()}
                                                    </span>
                                                </td>
                                                <td class="px-6 py-4">
                                                    <div class="flex items-center gap-2">
                                                        <span class="text-slate-900 font-bold block">{service.name}</span>
                                                        {if service.is_active {
                                                            view! {
                                                                <span class="px-1.5 py-0.5 bg-green-50 text-green-700 rounded text-[9px] font-bold border border-green-200">
                                                                    "Active"
                                                                </span>
                                                            }.into_any()
                                                        } else {
                                                            view! {
                                                                <span class="px-1.5 py-0.5 bg-slate-100 text-slate-500 rounded text-[9px] font-bold border border-slate-300">
                                                                    "Inactive"
                                                                </span>
                                                            }.into_any()
                                                        }}
                                                        {service.active_version_tag.map(|t| view! {
                                                            <span class="px-1.5 py-0.5 bg-emerald-50 text-emerald-700 rounded text-[10px] font-bold border border-emerald-200">
                                                                {t}
                                                            </span>
                                                        })}
                                                    </div>
                                                    <span class="text-slate-500 text-xs block">{service.description.unwrap_or_default()}</span>
                                                    {service.uuid.map(|uuid| view! {
                                                        <span class="text-slate-400 font-mono text-[10px] block mt-1">
                                                            <span class="font-semibold select-none">"UUID: "</span>
                                                            {uuid}
                                                        </span>
                                                    })}
                                                </td>
                                                <td class="px-6 py-4">
                                                    <div class="flex flex-wrap gap-1">
                                                        {service.tags.into_iter().map(|tag| view! {
                                                            <span class="px-2 py-0.5 bg-slate-100 text-slate-600 rounded-lg text-xs border border-slate-200">{tag}</span>
                                                        }).collect::<Vec<_>>()}
                                                    </div>
                                                </td>
                                                <td class="px-6 py-4 space-x-2">
                                                    <button
                                                        class="text-indigo-600 hover:text-indigo-800 font-semibold"
                                                        on:click=move |_| {
                                                            selected_service.set(Some(s_clone.clone()));
                                                            show_deploy_modal.set(true);
                                                            let s_id_dep = s_id_deploy.clone();
                                                            spawn_local(async move {
                                                                if let Ok(list) = api::get_envs(&s_id_dep).await {
                                                                    envs.set(list);
                                                                }
                                                            });
                                                            ide_files.set(vec![
                                                                ("src/main.rs".to_string(), "fn main() {\n    let input = rust_sdk::get_input().unwrap();\n    println!(\"{{\\\"status\\\":\\\"processed\\\",\\\"data\\\":{}}}\", input);\n}".to_string()),
                                                                ("Cargo.toml".to_string(), "[package]\nname = \"service\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nserde = { version = \"1.0\", features = [\"derive\"] }\nserde_json = \"1.0\"\ntokio = { version = \"1.0\", features = [\"full\"] }\nrust-sdk = { path = \"./rust-sdk\" }\n".to_string())
                                                            ]);
                                                            ide_active_file.set("src/main.rs".to_string());
                                                            version_tag.set("v1.0.0".to_string());
                                                            load_versions(s_id_deploy.clone());
                                                        }
                                                    >
                                                        "Deploy"
                                                    </button>
                                                    <span class="text-slate-300">"|"</span>
                                                    <button
                                                        class="text-indigo-600 hover:text-indigo-800 font-semibold"
                                                        on:click=move |_| {
                                                            editing_service_id.set(Some(s_id.clone()));
                                                            new_name.set(s_clone_edit.name.clone());
                                                            new_desc.set(s_clone_edit.description.clone().unwrap_or_default());
                                                            tags_list.set(s_clone_edit.tags.clone());
                                                            new_is_active.set(s_clone_edit.is_active);
                                                            
                                                            let s_act = s_clone_edit.on_success_action.clone().unwrap_or_else(|| "end".to_string());
                                                            let s_cfg = s_clone_edit.on_success_config.clone().unwrap_or_default();
                                                            success_action.set(s_act.clone());
                                                            success_config.set(s_cfg.clone());
                                                            if s_act == "key_event" {
                                                                if let Ok(json_cfg) = serde_json::from_str::<serde_json::Value>(&s_cfg) {
                                                                    success_ke_key.set(json_cfg.get("key").and_then(|k| k.as_str()).unwrap_or_default().to_string());
                                                                    success_ke_operator.set(json_cfg.get("operator").and_then(|o| o.as_str()).unwrap_or("==").to_string());
                                                                    success_ke_value.set(json_cfg.get("value").and_then(|v| v.as_str()).unwrap_or_default().to_string());
                                                                    success_ke_dest.set(json_cfg.get("destination_queue").and_then(|d| d.as_str()).unwrap_or_default().to_string());
                                                                }
                                                            } else {
                                                                success_ke_key.set(String::new());
                                                                success_ke_operator.set("==".to_string());
                                                                success_ke_value.set(String::new());
                                                                success_ke_dest.set(String::new());
                                                            }

                                                            let e_act = s_clone_edit.on_error_action.clone().unwrap_or_else(|| "end".to_string());
                                                            let e_cfg = s_clone_edit.on_error_config.clone().unwrap_or_default();
                                                            error_action.set(e_act.clone());
                                                            error_config.set(e_cfg.clone());
                                                            if e_act == "key_event" {
                                                                if let Ok(json_cfg) = serde_json::from_str::<serde_json::Value>(&e_cfg) {
                                                                    error_ke_key.set(json_cfg.get("key").and_then(|k| k.as_str()).unwrap_or_default().to_string());
                                                                    error_ke_operator.set(json_cfg.get("operator").and_then(|o| o.as_str()).unwrap_or("==").to_string());
                                                                    error_ke_value.set(json_cfg.get("value").and_then(|v| v.as_str()).unwrap_or_default().to_string());
                                                                    error_ke_dest.set(json_cfg.get("destination_queue").and_then(|d| d.as_str()).unwrap_or_default().to_string());
                                                                }
                                                            } else {
                                                                error_ke_key.set(String::new());
                                                                error_ke_operator.set("==".to_string());
                                                                error_ke_value.set(String::new());
                                                                error_ke_dest.set(String::new());
                                                            }
                                                        }
                                                    >
                                                        "Edit"
                                                    </button>
                                                    <span class="text-slate-300">"|"</span>
                                                    <button
                                                        class="text-red-600 hover:text-red-800 font-semibold"
                                                        on:click=move |_| {
                                                            let s_id_del = s_id_delete.clone();
                                                            spawn_local(async move {
                                                                match api::delete_service(&s_id_del).await {
                                                                     Ok(_) => reload(),
                                                                     Err(e) => {
                                                                         let window = web_sys::window().unwrap();
                                                                         let _ = window.alert_with_message(&e.to_string());
                                                                     }
                                                                }
                                                            });
                                                        }
                                                    >
                                                        "Delete"
                                                    </button>
                                                </td>
                                            </tr>
                                        }
                                    }).collect::<Vec<_>>().into_any()
                                }}
                            </tbody>
                        </table>
                        
                        // Pagination Controls
                        <div class="px-6 py-3 border-t border-slate-200 flex items-center justify-between bg-slate-50 text-xs font-semibold text-slate-500">
                            <div>
                                {move || {
                                    let total_len = services.get().len();
                                    let total_pages = (total_len + 5 - 1) / 5;
                                    format!("Page {} of {}", ms_page.get(), std::cmp::max(total_pages, 1))
                                }}
                            </div>
                            <div class="flex gap-2">
                                <button
                                    class="px-2 py-1 bg-white border border-slate-200 rounded text-slate-600 disabled:opacity-50 hover:bg-slate-50 transition"
                                    disabled=move || ms_page.get() <= 1
                                    on:click=move |_| ms_page.set(ms_page.get() - 1)
                                >
                                    "Previous"
                                </button>
                                <button
                                    class="px-2 py-1 bg-white border border-slate-200 rounded text-slate-600 disabled:opacity-50 hover:bg-slate-50 transition"
                                    disabled=move || {
                                        let total_len = services.get().len();
                                        let total_pages = (total_len + 5 - 1) / 5;
                                        ms_page.get() >= total_pages || total_pages == 0
                                    }
                                    on:click=move |_| ms_page.set(ms_page.get() + 1)
                                >
                                    "Next"
                                </button>
                            </div>
                        </div>
                    </div>
                </div>

                // Right: Create/Edit Service
                <div class="bg-white border border-slate-200 rounded-xl p-6 space-y-4 h-fit shadow-sm">
                    <h3 class="font-bold text-slate-800 text-lg">
                        {move || if let Some(id) = editing_service_id.get() { format!("Edit Service (#{id})") } else { "New Service".to_string() }}
                    </h3>
                    
                    <div>
                        <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">"Name"</label>
                        <input
                            type="text"
                            placeholder="DataTransformer"
                            class="w-full bg-slate-50 border border-slate-300 rounded-lg px-3 py-2 text-slate-900 outline-none focus:border-indigo-500 text-sm"
                            prop:value=new_name
                            on:input=move |ev| new_name.set(event_target_value(&ev))
                        />
                    </div>

                    <div>
                        <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">"Description"</label>
                        <textarea
                            placeholder="Transform input payload..."
                            class="w-full bg-slate-50 border border-slate-300 rounded-lg px-3 py-2 text-slate-900 outline-none focus:border-indigo-500 text-sm h-20"
                            prop:value=new_desc
                            on:input=move |ev| new_desc.set(event_target_value(&ev))
                        />
                    </div>

                    <div>
                        <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">"Tags (Press Enter)"</label>
                        <div class="flex flex-wrap gap-1 p-2 bg-slate-50 border border-slate-300 rounded-lg min-h-10">
                            {move || tags_list.get().into_iter().map(|tag| {
                                let tag_c = tag.clone();
                                view! {
                                    <span class="inline-flex items-center gap-1 px-2 py-0.5 bg-indigo-50 text-indigo-700 rounded text-xs border border-indigo-200">
                                        {tag.clone()}
                                        <button
                                            class="hover:text-red-500 font-bold"
                                            on:click=move |_| {
                                                let tag_c = tag_c.clone();
                                                tags_list.update(|list| list.retain(|x| x != &tag_c));
                                            }
                                        >
                                            "×"
                                        </button>
                                    </span>
                                }
                            }).collect::<Vec<_>>()}
                            <input
                                type="text"
                                placeholder="..."
                                class="bg-transparent text-sm text-slate-900 outline-none flex-1 min-w-16"
                                on:input=move |ev| tag_input.set(event_target_value(&ev))
                                on:keydown=move |ev| {
                                    if ev.key() == "Enter" {
                                        let val = tag_input.get().trim().to_string();
                                        if !val.is_empty() && !tags_list.get().contains(&val) {
                                            tags_list.update(|list| list.push(val.clone()));
                                            tag_input.set(String::new());
                                            if let Some(target) = ev.target() {
                                                let _ = target.unchecked_into::<web_sys::HtmlInputElement>().set_value("");
                                            }
                                        }
                                    }
                                }
                            />
                        </div>
                    </div>

                    <div>
                        <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">"Active Status"</label>
                        <label class="inline-flex items-center gap-2 text-sm text-slate-700 cursor-pointer">
                            <input
                                type="checkbox"
                                class="rounded border-slate-300 text-indigo-600 focus:ring-indigo-500 h-4 w-4"
                                prop:checked=new_is_active
                                on:change=move |ev| new_is_active.set(event_target_checked(&ev))
                            />
                            "Microservice is Active"
                        </label>
                    </div>

                    <div class="flex gap-2">
                        <button
                            class="flex-1 bg-black hover:bg-zinc-900 text-white font-semibold py-2 px-4 rounded-lg text-sm transition shadow-sm"
                            on:click=move |_| {
                                let name = new_name.get();
                                let description = Some(new_desc.get());
                                let tags = tags_list.get();
                                let active_status = new_is_active.get();

                                spawn_local(async move {
                                    let payload = MicroserviceDTO {
                                        id: editing_service_id.get(),
                                        uuid: None,
                                        name,
                                        language: "rust".to_string(),
                                        description,
                                        tags,
                                        active_version_id: None,
                                        active_version_tag: None,
                                        on_success_action: None,
                                        on_success_config: None,
                                        on_error_action: None,
                                        on_error_config: None,
                                        is_active: active_status,
                                        created_at: None,
                                        updated_at: None,
                                    };
                                    let res = if let Some(ref id) = editing_service_id.get() {
                                        api::update_service(id, payload).await.map(|_| ())
                                    } else {
                                        api::create_service(payload).await.map(|_| ())
                                    };

                                    match res {
                                        Ok(_) => {
                                            new_name.set(String::new());
                                            new_desc.set(String::new());
                                            tags_list.set(vec![]);
                                            new_is_active.set(true);
                                            editing_service_id.set(None);
                                            success_action.set("end".to_string());
                                            success_config.set(String::new());
                                            error_action.set("end".to_string());
                                            error_config.set(String::new());
                                            success_ke_key.set(String::new());
                                            success_ke_operator.set("==".to_string());
                                            success_ke_value.set(String::new());
                                            success_ke_dest.set(String::new());
                                            error_ke_key.set(String::new());
                                            error_ke_operator.set("==".to_string());
                                            error_ke_value.set(String::new());
                                            error_ke_dest.set(String::new());
                                            reload();
                                        }
                                        Err(e) => {
                                            if let Some(w) = web_sys::window() {
                                                let _ = w.alert_with_message(&format!("Error: {}", e));
                                            }
                                        }
                                    }
                                });
                            }
                        >
                            {move || if editing_service_id.get().is_some() { "Save Changes" } else { "Create Service" }}
                        </button>
                        
                        {move || if editing_service_id.get().is_some() {
                            view! {
                                <button
                                    class="bg-slate-200 hover:bg-slate-300 text-slate-700 font-semibold py-2 px-4 rounded-lg text-sm transition shadow-sm"
                                    on:click=move |_| {
                                        new_name.set(String::new());
                                        new_desc.set(String::new());
                                        tags_list.set(vec![]);
                                        editing_service_id.set(None);
                                        success_action.set("end".to_string());
                                        success_config.set(String::new());
                                        error_action.set("end".to_string());
                                        error_config.set(String::new());
                                    }
                                >
                                    "Cancel"
                                </button>
                            }.into_any()
                        } else {
                            view! { <div /> }.into_any()
                        }}
                    </div>
                </div>
            </div>

            // Bottom Section: Version Build / Deploy Editor (Modal overlay)
            {move || {
                if !show_deploy_modal.get() {
                    return view! { <div /> }.into_any();
                }
                selected_service.get().map(|service| {
                    let s_id = service.id.clone().unwrap_or_default();
                    let s_id_c_for_envs = s_id.clone();
                    view! {
                        <div class="fixed inset-0 z-40 bg-slate-900/60 flex items-center justify-center p-6 backdrop-blur-sm overflow-y-auto">
                            <div class="bg-white border border-slate-200 rounded-2xl max-w-7xl w-full p-8 space-y-6 shadow-2xl overflow-y-auto max-h-[92vh]">
                                <div class="flex justify-between items-center border-b border-slate-200 pb-4">
                                    <h3 class="font-bold text-slate-900 text-xl">{format!("Deploy version for: {} (#{})", service.name, s_id)}</h3>
                                    <button
                                        class="bg-slate-200 hover:bg-slate-300 text-slate-700 font-bold px-4 py-2 rounded-lg transition"
                                        on:click=move |_| {
                                            show_deploy_modal.set(false);
                                            selected_service.set(None);
                                        }
                                    >
                                        "Close Editor"
                                    </button>
                                </div>
                                     <div class="flex border-b border-slate-200 gap-6 mb-2">
                        <button
                            class=move || format!("pb-3 font-bold text-sm border-b-2 transition flex items-center gap-2 {}", if deploy_modal_tab.get() == "deploy" { "border-indigo-600 text-indigo-600 font-bold" } else { "border-transparent text-slate-400 hover:text-slate-700" })
                            on:click=move |_| deploy_modal_tab.set("deploy".to_string())
                        >
                            <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4" />
                            </svg>
                            <span>"Deploy"</span>
                        </button>
                        <button
                            class=move || format!("pb-3 font-bold text-sm border-b-2 transition flex items-center gap-2 {}", if deploy_modal_tab.get() == "variables" { "border-indigo-600 text-indigo-600 font-bold" } else { "border-transparent text-slate-400 hover:text-slate-700" })
                            on:click=move |_| deploy_modal_tab.set("variables".to_string())
                        >
                            <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4" />
                            </svg>
                            <span>"Variables"</span>
                            {move || {
                                let count = envs.get().len();
                                if count > 0 {
                                    view! {
                                        <span class="px-1.5 py-0.5 rounded-full text-[10px] font-bold bg-slate-100 text-slate-700 border border-slate-200">
                                            {count}
                                        </span>
                                    }.into_any()
                                } else {
                                    view! { <span /> }.into_any()
                                }
                            }}
                        </button>
                    </div>

                    {
                        let s_id_c = s_id.clone();
                        let load_versions = load_versions.clone();
                        move || {
                            let s_id_c = s_id_c.clone();
                            let load_versions = load_versions.clone();
                            if deploy_modal_tab.get() == "variables" {
                            view! {
                                <div class="space-y-4 pt-2">
                                    <h4 class="font-bold text-slate-900 text-md">"Manage Environment Variables"</h4>
                                    <div class="grid grid-cols-1 md:grid-cols-3 gap-6">
                                        // Left: Envs List
                                        <div class="md:col-span-2 space-y-2">
                                            <div class="border border-slate-200 rounded-xl overflow-hidden text-xs bg-slate-50">
                                                <table class="w-full text-left">
                                                    <thead class="bg-slate-100 text-[10px] font-semibold text-slate-500 uppercase border-b border-slate-200">
                                                        <tr>
                                                            <th class="px-4 py-2">"Env Name"</th>
                                                            <th class="px-4 py-2">"Config JSON"</th>
                                                            <th class="px-4 py-2">"Default?"</th>
                                                            <th class="px-4 py-2">"Action"</th>
                                                        </tr>
                                                    </thead>
                                                    <tbody class="divide-y divide-slate-200">
                                                        {
                                                            let s_id_c = s_id_c_for_envs.clone();
                                                            move || {
                                                                let list = envs.get();
                                                                if list.is_empty() {
                                                                    view! {
                                                                        <tr>
                                                                            <td colspan="4" class="px-4 py-4 text-center text-slate-400">
                                                                                "No environments configured yet"
                                                                            </td>
                                                                        </tr>
                                                                    }.into_any()
                                                                } else {
                                                                    let s_id_c = s_id_c.clone();
                                                                    list.into_iter().map(|env| {
                                                                        let s_id_c = s_id_c.clone();
                                                                        let is_def = env.is_default;
                                                                        let env_clone = env.clone();
                                                                        view! {
                                                                            <tr class="hover:bg-slate-100/50">
                                                                                <td class="px-4 py-2 font-bold text-slate-800">{env.name}</td>
                                                                                <td class="px-4 py-2 font-mono text-slate-600 max-w-xs truncate">
                                                                                    {serde_json::to_string(&env.config).unwrap_or_default()}
                                                                                </td>
                                                                                <td class="px-4 py-2">
                                                                                    {if is_def {
                                                                                        view! { <span class="px-1.5 py-0.5 bg-emerald-50 text-emerald-700 font-bold border border-emerald-200 rounded">"Default"</span> }.into_any()
                                                                                    } else {
                                                                                        let s_id_c = s_id_c.clone();
                                                                                        view! {
                                                                                            <button
                                                                                                class="text-indigo-600 hover:text-indigo-800 font-semibold"
                                                                                                on:click=move |_| {
                                                                                                    let s_id_c = s_id_c.clone();
                                                                                                    let mut payload = env_clone.clone();
                                                                                                    payload.is_default = true;
                                                                                                    spawn_local(async move {
                                                                                                        let _ = api::create_env(&s_id_c, payload).await;
                                                                                                        if let Ok(list) = api::get_envs(&s_id_c).await {
                                                                                                            envs.set(list);
                                                                                                        }
                                                                                                    });
                                                                                                }
                                                                                            >
                                                                                                "Make Default"
                                                                                            </button>
                                                                                        }.into_any()
                                                                                    }}
                                                                                </td>
                                                                                <td class="px-4 py-2">
                                                                                    <button
                                                                                        class="text-red-600 hover:text-red-800 font-semibold"
                                                                                        on:click=move |_| {
                                                                                            let s_id_c = s_id_c.clone();
                                                                                            let env_id_val = env.id.clone().unwrap_or_default();
                                                                                            spawn_local(async move {
                                                                                                let _ = api::delete_env_by_id(&s_id_c, &env_id_val).await;
                                                                                                if let Ok(list) = api::get_envs(&s_id_c).await {
                                                                                                    envs.set(list);
                                                                                                }
                                                                                            });
                                                                                        }
                                                                                    >
                                                                                        "Delete"
                                                                                    </button>
                                                                                </td>
                                                                            </tr>
                                                                        }
                                                                    }).collect::<Vec<_>>().into_any()
                                                                }
                                                            }
                                                        }
                                                    </tbody>
                                                </table>
                                            </div>
                                        </div>

                                        // Right: Add Env Form
                                        <div class="bg-slate-50 border border-slate-200 rounded-xl p-4 space-y-3 text-xs">
                                            <h5 class="font-bold text-slate-800 text-sm">"Add Environment"</h5>
                                            <div>
                                                <label class="block text-[10px] font-semibold text-slate-500 uppercase tracking-wider mb-1">"Env Name (e.g. dev, prod)"</label>
                                                <input
                                                    type="text"
                                                    placeholder="prod"
                                                    class="w-full bg-white border border-slate-300 rounded-lg px-2.5 py-1.5 text-slate-900 outline-none text-xs"
                                                    on:input=move |ev| new_env_name.set(event_target_value(&ev))
                                                    prop:value=new_env_name
                                                />
                                            </div>
                                            <div>
                                                <label class="block text-[10px] font-semibold text-slate-500 uppercase tracking-wider mb-1">"JSON Config variables"</label>
                                                <textarea
                                                    placeholder=r#"{"API_KEY": "secret", "PORT": "8080"}"#
                                                    class="w-full h-20 bg-white border border-slate-300 rounded-lg px-2.5 py-1.5 text-slate-900 outline-none text-xs font-mono"
                                                    on:input=move |ev| new_env_config.set(event_target_value(&ev))
                                                    prop:value=new_env_config
                                                />
                                            </div>
                                            <div class="flex items-center gap-2">
                                                <input
                                                    type="checkbox"
                                                    id="default_env_chk"
                                                    on:change=move |ev| new_env_is_default.set(event_target_checked(&ev))
                                                    prop:checked=new_env_is_default
                                                />
                                                <label for="default_env_chk" class="text-[10px] font-semibold text-slate-500 uppercase tracking-wider select-none cursor-pointer">"Set as Default"</label>
                                            </div>
                                            <button
                                                class="w-full bg-black hover:bg-zinc-900 text-white font-semibold py-1.5 rounded-lg text-xs transition shadow-sm"
                                                on:click={
                                                    let s_id_c = s_id_c_for_envs.clone();
                                                    move |_| {
                                                        let s_id_c = s_id_c.clone();
                                                        let name = new_env_name.get();
                                                        let config_raw = new_env_config.get();
                                                        let is_def = new_env_is_default.get();
                                                        
                                                        if name.is_empty() { return; }
                                                        
                                                        let config_trimmed = config_raw.trim();
                                                        let config_val: serde_json::Value = if config_trimmed.is_empty() {
                                                            serde_json::json!({})
                                                        } else {
                                                            match serde_json::from_str(config_trimmed) {
                                                                Ok(v) => v,
                                                                Err(_) => {
                                                                    if let Some(w) = web_sys::window() {
                                                                        let _ = w.alert_with_message("Invalid JSON configuration object");
                                                                    }
                                                                    return;
                                                                }
                                                            }
                                                        };
                                                        
                                                        spawn_local(async move {
                                                            let payload = MicroserviceEnvDTO {
                                                                id: None,
                                                                microservice_id: Some(s_id_c.clone()),
                                                                name,
                                                                config: config_val,
                                                                is_default: is_def,
                                                            };
                                                            if let Ok(_) = api::create_env(&s_id_c, payload).await {
                                                                new_env_name.set(String::new());
                                                                new_env_config.set(String::new());
                                                                new_env_is_default.set(false);
                                                                if let Ok(list) = api::get_envs(&s_id_c).await {
                                                                    envs.set(list);
                                                                }
                                                            }
                                                        });
                                                    }
                                                }
                                            >
                                                "Save Environment"
                                            </button>
                                        </div>
                                    </div>
                                </div>
                            }.into_any()
                        } else {
                            view! {
                                <div class="grid grid-cols-1 lg:grid-cols-3 gap-8">
                                    // Left: Editor
                                    <div class="lg:col-span-2 space-y-4">
                                        <div class="grid grid-cols-1 md:grid-cols-4 gap-4">
                                            <div>
                                                <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">"Version Tag"</label>
                                                <input
                                                    type="text"
                                                    placeholder="1.0.0"
                                                    class="w-full bg-slate-50 border border-slate-300 rounded-lg px-3 py-2 text-slate-900 outline-none focus:border-indigo-500 text-sm"
                                                    prop:value=version_tag
                                                    on:input=move |ev| version_tag.set(event_target_value(&ev))
                                                />
                                            </div>
                                        </div>
                                        <div class="flex gap-4 border-b border-slate-200 pb-2">
                                            <button
                                                class=move || format!("px-4 py-2 font-bold text-sm border-b-2 transition {}", if source_type.get() == "textarea" { "border-indigo-600 text-indigo-600" } else { "border-transparent text-slate-500 hover:text-slate-800" })
                                                on:click=move |_| source_type.set("textarea".to_string())
                                            >
                                                "Visual IDE Editor"
                                            </button>
                                            <button
                                                class=move || format!("px-4 py-2 font-bold text-sm border-b-2 transition {}", if source_type.get() == "zip" { "border-indigo-600 text-indigo-600" } else { "border-transparent text-slate-500 hover:text-slate-800" })
                                                on:click=move |_| {
                                                    source_type.set("zip".to_string());
                                                    version_code.set(String::new());
                                                }
                                            >
                                                "Upload ZIP File"
                                            </button>
                                            <button
                                                class=move || format!("px-4 py-2 font-bold text-sm border-b-2 transition {}", if source_type.get() == "github" { "border-indigo-600 text-indigo-600" } else { "border-transparent text-slate-500 hover:text-slate-800" })
                                                on:click=move |_| {
                                                    source_type.set("github".to_string());
                                                    version_code.set(String::new());
                                                }
                                            >
                                                "GitHub Repository"
                                            </button>
                                        </div>

                                        {move || match source_type.get().as_str() {
                                            "zip" => view! {
                                                <div class="bg-slate-50 border border-slate-200 rounded-lg p-6 space-y-4">
                                                    <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider">"Select ZIP File"</label>
                                                    <input
                                                        type="file"
                                                        accept=".zip"
                                                        class="w-full text-slate-700 file:mr-4 file:py-2 file:px-4 file:rounded-lg file:border-0 file:text-sm file:font-semibold file:bg-indigo-50 file:text-indigo-700 hover:file:bg-indigo-100"
                                                        on:change=move |ev| {
                                                            use wasm_bindgen::JsCast;
                                                            let file_input: web_sys::HtmlInputElement = ev.target().unwrap().unchecked_into();
                                                            if let Some(files) = file_input.files() {
                                                                if let Some(file) = files.get(0) {
                                                                    let reader = web_sys::FileReader::new().unwrap();
                                                                    let reader_c = reader.clone();
                                                                    let onload = Closure::<dyn FnMut()>::new(move || {
                                                                        let result = reader_c.result().unwrap();
                                                                        let array_buffer = js_sys::ArrayBuffer::from(result);
                                                                        let uint8_array = js_sys::Uint8Array::new(&array_buffer);
                                                                        let bytes = uint8_array.to_vec();
                                                                        use base64::Engine;
                                                                        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                                                                        version_code.set(b64);
                                                                    });
                                                                    reader.set_onload(Some(onload.as_ref().unchecked_ref()));
                                                                    reader.read_as_array_buffer(&file).unwrap();
                                                                    onload.forget();
                                                                }
                                                            }
                                                        }
                                                    />
                                                </div>
                                            }.into_any(),
                                            "github" => view! {
                                                <div class="bg-slate-50 border border-slate-200 rounded-lg p-6 space-y-4">
                                                    <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider">"GitHub Repository URL"</label>
                                                    <input
                                                        type="text"
                                                        placeholder="https://github.com/owner/repo#branch"
                                                        class="w-full bg-white border border-slate-300 rounded-lg px-3 py-2 text-slate-900 outline-none focus:border-indigo-500 text-sm"
                                                        on:input=move |ev| version_code.set(event_target_value(&ev))
                                                        prop:value=version_code
                                                    />
                                                    <p class="text-xs text-slate-500">"Clone URL with optional branch specified as #branch_name"</p>
                                                </div>
                                            }.into_any(),
                                            _ => view! {
                                                <div class="border border-slate-200 rounded-lg overflow-hidden flex h-96 bg-slate-900 text-slate-100 font-mono text-xs shadow-sm">
                                                    <div class="w-1/4 bg-slate-950 border-r border-slate-800 flex flex-col justify-between">
                                                        <div class="p-3 space-y-1 overflow-y-auto">
                                                            <div class="text-[9px] font-extrabold uppercase tracking-wider text-slate-500 mb-2">"Files"</div>
                                                            {move || ide_files.get().into_iter().map(|(path, _)| {
                                                                let p_active = path.clone();
                                                                let p_click = path.clone();
                                                                let active = move || ide_active_file.get() == p_active;
                                                                view! {
                                                                    <button
                                                                        class=move || format!("w-full text-left px-2 py-1.5 rounded transition flex items-center gap-2 {}", if active() { "bg-indigo-600/30 text-indigo-400 border-l-2 border-indigo-500" } else { "text-slate-400 hover:bg-slate-800 hover:text-slate-200" })
                                                                        on:click=move |_| ide_active_file.set(p_click.clone())
                                                                    >
                                                                        <span class="opacity-75">"📄"</span>
                                                                        {path}
                                                                    </button>
                                                                }
                                                            }).collect::<Vec<_>>()}
                                                        </div>
                                                        <div class="p-3 border-t border-slate-800 space-y-2">
                                                            {move || if ide_show_new_file_input.get() {
                                                                view! {
                                                                    <div class="flex flex-col gap-1.5">
                                                                        <input
                                                                            type="text"
                                                                            placeholder="e.g. src/helper.rs"
                                                                            class="w-full bg-slate-900 border border-slate-800 rounded px-2 py-1 text-slate-200 outline-none focus:border-indigo-500 placeholder-slate-600 font-sans"
                                                                            prop:value=ide_new_file_name
                                                                            on:input=move |ev| ide_new_file_name.set(event_target_value(&ev))
                                                                        />
                                                                        <div class="flex gap-1">
                                                                            <button
                                                                                class="flex-1 bg-black hover:bg-zinc-900 text-white rounded py-1 font-semibold text-[10px] font-sans"
                                                                                on:click=move |_| {
                                                                                    let new_path = ide_new_file_name.get().trim().to_string();
                                                                                    if !new_path.is_empty() {
                                                                                        let mut list = ide_files.get();
                                                                                        if !list.iter().any(|(p, _)| p == &new_path) {
                                                                                            list.push((new_path.clone(), String::new()));
                                                                                            ide_files.set(list);
                                                                                            ide_active_file.set(new_path);
                                                                                        }
                                                                                        ide_new_file_name.set(String::new());
                                                                                        ide_show_new_file_input.set(false);
                                                                                    }
                                                                                }
                                                                            >
                                                                                "Add"
                                                                            </button>
                                                                            <button
                                                                                class="bg-slate-800 hover:bg-slate-700 text-slate-300 rounded px-2 py-1 text-[10px] font-sans"
                                                                                on:click=move |_| ide_show_new_file_input.set(false)
                                                                            >
                                                                                "Cancel"
                                                                            </button>
                                                                        </div>
                                                                    </div>
                                                                }.into_any()
                                                            } else {
                                                                view! {
                                                                    <button
                                                                        class="w-full bg-slate-900 hover:bg-slate-800 border border-slate-800 rounded py-1.5 text-slate-400 hover:text-slate-200 font-semibold text-center block font-sans"
                                                                        on:click=move |_| ide_show_new_file_input.set(true)
                                                                    >
                                                                        "+ Add File"
                                                                    </button>
                                                                }.into_any()
                                                            }}
                                                        </div>
                                                    </div>
                                                    <div class="flex-1 flex flex-col">
                                                        <div class="h-9 bg-slate-950 border-b border-slate-800 flex items-center px-4 justify-between">
                                                            <span class="text-indigo-400 font-bold">{move || ide_active_file.get()}</span>
                                                            <button
                                                                class="text-red-500 hover:text-red-400 text-[10px] font-sans font-bold"
                                                                on:click=move |_| {
                                                                    let active = ide_active_file.get();
                                                                    if active != "src/main.rs" && active != "Cargo.toml" {
                                                                        let mut list = ide_files.get();
                                                                        list.retain(|(p, _)| p != &active);
                                                                        ide_files.set(list);
                                                                        ide_active_file.set("src/main.rs".to_string());
                                                                    }
                                                                }
                                                            >
                                                                "Delete File"
                                                            </button>
                                                        </div>
                                                        <textarea
                                                            class="flex-1 w-full bg-slate-900 text-slate-100 p-4 font-mono text-xs outline-none resize-none"
                                                            prop:value=move || {
                                                                let active = ide_active_file.get();
                                                                ide_files.get().into_iter().find(|(p, _)| p == &active).map(|(_, c)| c).unwrap_or_default()
                                                            }
                                                            on:input=move |ev| {
                                                                let active = ide_active_file.get();
                                                                let new_content = event_target_value(&ev);
                                                                let mut list = ide_files.get();
                                                                if let Some(idx) = list.iter().position(|(p, _)| p == &active) {
                                                                    list[idx].1 = new_content;
                                                                    ide_files.set(list);
                                                                }
                                                            }
                                                        />
                                                    </div>
                                                </div>
                                            }.into_any()
                                        }}

                                        // Real-time Build Logs Terminal
                                        {move || {
                                            let status = build_status.get();
                                            let logs = build_logs_stream.get();
                                            if status.is_some() || !logs.is_empty() {
                                                view! {
                                                    <div class="bg-slate-900 border border-slate-800 rounded-xl p-4 font-mono text-xs space-y-2">
                                                        <div class="flex items-center justify-between border-b border-slate-800 pb-2">
                                                            <div class="flex items-center gap-2">
                                                                <span class="w-2.5 h-2.5 rounded-full bg-emerald-500 animate-pulse"></span>
                                                                <span class="text-slate-300 font-bold">"Live Docker Build Output"</span>
                                                            </div>
                                                            {status.map(|st| {
                                                                let st_txt = st.clone();
                                                                view! {
                                                                    <span class=format!("px-2 py-0.5 rounded text-[10px] font-bold uppercase {}", match st.as_str() {
                                                                        "success" => "bg-emerald-500/20 text-emerald-400 border border-emerald-500/30",
                                                                        "failed" => "bg-red-500/20 text-red-400 border border-red-500/30",
                                                                        _ => "bg-indigo-500/20 text-indigo-400 border border-indigo-500/30"
                                                                    })>
                                                                        {st_txt}
                                                                    </span>
                                                                }
                                                            })}
                                                        </div>
                                                        <div class="text-slate-300 max-h-48 overflow-y-auto whitespace-pre-wrap select-text">
                                                            {if logs.is_empty() { "Starting Docker build process...".to_string() } else { logs }}
                                                        </div>
                                                    </div>
                                                }.into_any()
                                            } else {
                                                view! { <div /> }.into_any()
                                            }
                                        }}

                                        <div class="flex gap-4 items-center">
                                            <button
                                                class="bg-indigo-600 hover:bg-indigo-500 text-white font-semibold py-2 px-6 rounded-lg text-sm transition shadow-sm disabled:opacity-50 flex items-center gap-2"
                                                disabled=build_in_progress
                                                on:click={
                                                    let s_id_c = s_id_c.clone();
                                                    let load_versions = load_versions.clone();
                                                    move |_| {
                                                        let s_id = s_id_c.clone();
                                                        let tag = version_tag.get();
                                                        let st = source_type.get();
                                                        
                                                        let (raw_source, actual_st) = if st == "zip" || st == "github" {
                                                            (version_code.get(), st)
                                                        } else {
                                                            let files = ide_files.get();
                                                            let mut map = std::collections::HashMap::new();
                                                            for (f, c) in files {
                                                                map.insert(f, c);
                                                            }
                                                            (serde_json::to_string(&map).unwrap_or_default(), "textarea".to_string())
                                                        };
                                                        
                                                        if tag.is_empty() || raw_source.is_empty() { return; }

                                                        let load_versions = load_versions.clone();
                                                        build_in_progress.set(true);
                                                        build_status.set(Some("building".to_string()));
                                                        build_logs_stream.set("Sending source files to build daemon...\n".to_string());
                                                        
                                                        // Start real-time build logs poller
                                                        let s_id_log_poll = s_id.clone();
                                                        spawn_local(async move {
                                                            use gloo_timers::future::TimeoutFuture;
                                                            while build_in_progress.get() {
                                                                if let Ok(logs) = api::get_build_logs(&s_id_log_poll).await {
                                                                    build_logs_stream.set(logs);
                                                                }
                                                                TimeoutFuture::new(500).await;
                                                            }
                                                        });

                                                        spawn_local(async move {
                                                            let parsed_num = tag.chars()
                                                                .filter(|c| c.is_ascii_digit())
                                                                .collect::<String>()
                                                                .parse::<i64>()
                                                                .unwrap_or(1);

                                                            let payload = MicroserviceVersionDTO {
                                                                id: None,
                                                                microservice_id: s_id.clone(),
                                                                version_number: parsed_num,
                                                                version_tag: tag,
                                                                source_type: actual_st,
                                                                source_code: raw_source,
                                                                container_image_tag: None,
                                                                container_id: None,
                                                                status: "draft".to_string(),
                                                                changelog: Some("Release deployment".to_string()),
                                                                error_message: None,
                                                                created_at: None,
                                                            };
                                                            
                                                            match api::create_version(&s_id, payload).await {
                                                                Ok(_) => {
                                                                    build_status.set(Some("success".to_string()));
                                                                    build_logs_stream.set(format!("{}\nBuild succeeded! Docker image created and deployed.", build_logs_stream.get()));
                                                                    build_in_progress.set(false);
                                                                    if let Ok(logs) = api::get_build_logs(&s_id).await {
                                                                        build_logs_stream.set(logs);
                                                                    }
                                                                    load_versions(s_id.clone());
                                                                    reload();
                                                                }
                                                                Err(e) => {
                                                                    build_status.set(Some("failed".to_string()));
                                                                    build_logs_stream.set(format!("{}\nBuild failed:\n{}", build_logs_stream.get(), e));
                                                                    build_in_progress.set(false);
                                                                    if let Ok(logs) = api::get_build_logs(&s_id).await {
                                                                        build_logs_stream.set(logs);
                                                                    }
                                                                    load_versions(s_id.clone());
                                                                }
                                                            }
                                                        });
                                                    }
                                                }
                                            >
                                                "Compile & Deploy"
                                            </button>

                                            {
                                                move || {
                                                    if let Some(ref sel) = selected_service.get() {
                                                        if let Some(ref active_v_id) = sel.active_version_id {
                                                            let active_v_id = active_v_id.clone();
                                                            view! {
                                                                <button
                                                                    class="bg-slate-800 hover:bg-slate-700 text-white font-semibold py-2 px-6 rounded-lg text-sm transition shadow-sm border border-slate-700"
                                                                    on:click=move |_| {
                                                                        testing_version_id.set(Some(active_v_id.clone()));
                                                                        test_result_output.set(None);
                                                                    }
                                                                >
                                                                    "Test Active Version"
                                                                </button>
                                                            }.into_any()
                                                        } else {
                                                            view! { <div /> }.into_any()
                                                        }
                                                    } else {
                                                        view! { <div /> }.into_any()
                                                    }
                                                }
                                            }
                                        </div>
                                    </div>

                                    // Right: Version History list
                                    <div class="space-y-4">
                                        <h4 class="font-bold text-slate-800 text-md">"Version History"</h4>
                                        <div class="border border-slate-200 rounded-xl overflow-hidden bg-slate-50 text-sm">
                                            <table class="w-full text-left">
                                                <thead class="bg-slate-100 text-xs font-semibold text-slate-500 uppercase border-b border-slate-200">
                                                    <tr>
                                                        <th class="px-4 py-2">"Tag / Status"</th>
                                                        <th class="px-4 py-2">"Action"</th>
                                                    </tr>
                                                </thead>
                                                <tbody class="divide-y divide-slate-200 text-xs">
                                                    {move || {
                                                        let all_vers = service_versions.get();
                                                        let total_len = all_vers.len();
                                                        let page = version_page.get();
                                                        let page_size = 5;
                                                        let start = (page - 1) * page_size;
                                                        let end = std::cmp::min(page * page_size, total_len);

                                                        if total_len == 0 {
                                                            return view! {
                                                                <tr>
                                                                    <td colspan="2" class="px-4 py-6 text-center text-slate-400">
                                                                        "No versions deployed yet"
                                                                    </td>
                                                                </tr>
                                                            }.into_any();
                                                        }

                                                        let paged_vers = if start < total_len { all_vers[start..end].to_vec() } else { Vec::new() };
                                                        let s_id_c_for_rows = s_id_c.clone();
                                                        let load_versions_for_rows = load_versions.clone();
                                                        paged_vers.into_iter().map(move |ver| {
                                                            let v_id = ver.id.clone().unwrap_or_default();
                                                            let v_tag_c = ver.version_tag.clone();
                                                            let v_status = ver.status.clone();
                                                            let s_id_c = s_id_c_for_rows.clone();
                                                            let load_versions = load_versions_for_rows.clone();
                                                            view! {
                                                                <tr class="hover:bg-slate-200/30">
                                                                    <td class="px-4 py-3 space-y-1">
                                                                        <span class="font-semibold text-slate-900 block">{v_tag_c}</span>
                                                                        <div class="flex flex-wrap items-center gap-1.5">
                                                                            <VersionStatusBadge version_id=v_id.clone() />
                                                                        </div>
                                                                        {
                                                                            let err_msg = ver.error_message.clone();
                                                                            let v_status_c = v_status.clone();
                                                                            move || err_msg.as_ref().map(|err| view! {
                                                                                <details class="mt-1">
                                                                                    <summary class="cursor-pointer text-[10px] font-bold text-slate-500 hover:text-slate-700 outline-none select-none">
                                                                                        {if v_status_c == "failed" { "Show Build Errors" } else { "Show Build Logs" }}
                                                                                    </summary>
                                                                                    <div class="bg-slate-900 border border-slate-800 rounded p-2 mt-1 text-[10px] font-mono text-slate-200 max-w-xs break-words overflow-x-auto max-h-[150px] whitespace-pre">
                                                                                        {err.clone()}
                                                                                    </div>
                                                                                </details>
                                                                            })
                                                                        }
                                                                    </td>
                                                                    <td class="px-4 py-3">
                                                                         {
                                                                             let v_id = v_id.clone();
                                                                             let v_id_check = v_id.clone();
                                                                             let s_id_c_outer = s_id_c.clone();
                                                                             let load_versions_outer = load_versions.clone();
                                                                             let is_active = move || {
                                                                                 if let Some(ref sel) = selected_service.get() {
                                                                                     sel.active_version_id.as_ref() == Some(&v_id_check)
                                                                                 } else {
                                                                                     false
                                                                                 }
                                                                             };
                                                                             move || {
                                                                                  let v_id_test = v_id.clone();
                                                                                  let v_id_test2 = v_id.clone();
                                                                                  let s_id_inner1 = s_id_c_outer.clone();
                                                                                  let s_id_inner2 = s_id_c_outer.clone();
                                                                                  let load_v_inner1 = load_versions_outer.clone();
                                                                                  let load_v_inner2 = load_versions_outer.clone();
                                                                                  let v_id_activate = v_id.clone();

                                                                                  if is_active() {
                                                                                      view! {
                                                                                          <div class="flex gap-1.5">
                                                                                              <button
                                                                                                  class="bg-indigo-50 hover:bg-indigo-100 border border-indigo-200 px-2 py-1 rounded font-semibold text-indigo-600 transition text-[10px]"
                                                                                                  on:click=move |_| {
                                                                                                      testing_version_id.set(Some(v_id_test.clone()));
                                                                                                      test_result_output.set(None);
                                                                                                  }
                                                                                              >
                                                                                                  "Test"
                                                                                              </button>
                                                                                              <button
                                                                                                  class="bg-red-50 hover:bg-red-100 border border-red-200 px-2 py-1 rounded font-semibold text-red-600 transition text-[10px]"
                                                                                                  on:click=move |_| {
                                                                                                      let s_id_c = s_id_inner1.clone();
                                                                                                      let load_versions = load_v_inner1.clone();
                                                                                                      spawn_local(async move {
                                                                                                          let _ = api::rollback_version(&s_id_c, "null").await;
                                                                                                          load_versions(s_id_c);
                                                                                                          reload();
                                                                                                      });
                                                                                                  }
                                                                                              >
                                                                                                  "Deactivate"
                                                                                              </button>
                                                                                          </div>
                                                                                      }.into_any()
                                                                                  } else {
                                                                                      view! {
                                                                                          <div class="flex gap-1.5">
                                                                                              <button
                                                                                                  class="bg-indigo-50 hover:bg-indigo-100 border border-indigo-200 px-2 py-1 rounded font-semibold text-indigo-600 transition text-[10px]"
                                                                                                  on:click=move |_| {
                                                                                                      testing_version_id.set(Some(v_id_test2.clone()));
                                                                                                      test_result_output.set(None);
                                                                                                  }
                                                                                              >
                                                                                                  "Test"
                                                                                              </button>
                                                                                              <button
                                                                                                  class="bg-white hover:bg-slate-100 border border-slate-200 px-2 py-1 rounded font-semibold text-indigo-600 transition text-[10px]"
                                                                                                  on:click=move |_| {
                                                                                                      let s_id_c = s_id_inner2.clone();
                                                                                                      let v_id = v_id_activate.clone();
                                                                                                      let load_versions = load_v_inner2.clone();
                                                                                                      spawn_local(async move {
                                                                                                          let _ = api::rollback_version(&s_id_c, &v_id).await;
                                                                                                          load_versions(s_id_c);
                                                                                                          reload();
                                                                                                      });
                                                                                                  }
                                                                                              >
                                                                                                  "Activate"
                                                                                              </button>
                                                                                          </div>
                                                                                      }.into_any()
                                                                                  }
                                                                              }
                                                                          }
                                                                     </td>
                                                                </tr>
                                                            }
                                                        }).collect::<Vec<_>>().into_any()
                                                    }}
                                                </tbody>
                                            </table>
                                            
                                            // Pagination Controls
                                            <div class="px-4 py-2 border-t border-slate-200 flex items-center justify-between bg-slate-100 text-xs font-semibold text-slate-500">
                                                <div>
                                                    {move || {
                                                        let total_len = service_versions.get().len();
                                                        let total_pages = (total_len + 5 - 1) / 5;
                                                        format!("Page {} of {}", version_page.get(), std::cmp::max(total_pages, 1))
                                                    }}
                                                </div>
                                                <div class="flex gap-1.5">
                                                    <button
                                                        class="px-2 py-0.5 bg-white border border-slate-200 rounded text-slate-600 disabled:opacity-50 hover:bg-slate-50 transition text-[11px]"
                                                        disabled=move || version_page.get() <= 1
                                                        on:click=move |_| version_page.set(version_page.get() - 1)
                                                    >
                                                        "Prev"
                                                    </button>
                                                    <button
                                                        class="px-2 py-0.5 bg-white border border-slate-200 rounded text-slate-600 disabled:opacity-50 hover:bg-slate-50 transition text-[11px]"
                                                        disabled=move || {
                                                            let total_len = service_versions.get().len();
                                                            let total_pages = (total_len + 5 - 1) / 5;
                                                            version_page.get() >= total_pages || total_pages == 0
                                                        }
                                                        on:click=move |_| version_page.set(version_page.get() + 1)
                                                    >
                                                        "Next"
                                                    </button>
                                                </div>
                                            </div>
                                        </div>
                                    </div>
                                </div>
                            }.into_any()
                        }
                    }}
                            </div>
                        </div>
                    }
                }).into_any()
            }}

            // Version Test Modal overlay
            {move || testing_version_id.get().map(|v_id| {
                let v_id_c = v_id.clone();
                view! {
                    <div class="fixed inset-0 bg-slate-900/50 backdrop-blur-sm flex items-center justify-center p-6 z-50">
                        <div class="bg-white border border-slate-200 rounded-2xl max-w-lg w-full p-6 space-y-4 shadow-2xl">
                            <div class="flex justify-between items-center border-b border-slate-100 pb-3">
                                <h3 class="font-bold text-slate-800 text-lg">"Test Microservice Version"</h3>
                                <button
                                    class="text-slate-400 hover:text-slate-600 font-bold"
                                    on:click=move |_| {
                                        testing_version_id.set(None);
                                        test_result_output.set(None);
                                    }
                                >
                                    "✕"
                                </button>
                            </div>

                            <div class="space-y-3">
                                <div>
                                    <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">"Test JSON Payload"</label>
                                    <textarea
                                        class="w-full h-32 bg-slate-50 border border-slate-300 rounded-lg p-3 text-slate-900 font-mono text-xs outline-none focus:border-indigo-500"
                                        on:input=move |ev| test_payload_input.set(event_target_value(&ev))
                                    >
                                        {test_payload_input.get()}
                                    </textarea>
                                </div>

                                {move || if test_in_progress.get() {
                                    view! {
                                        <div class="p-3 bg-slate-50 border border-slate-200 rounded-lg text-slate-500 text-xs font-semibold text-center animate-pulse">
                                            "Executing Docker container test..."
                                        </div>
                                    }.into_any()
                                } else {
                                    view! { <div /> }.into_any()
                                }}

                                {move || test_result_output.get().map(|res| view! {
                                    <div class="space-y-2">
                                        <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider">"Execution Output"</label>
                                        <pre class="bg-slate-900 text-emerald-400 rounded-lg p-4 font-mono text-[10px] overflow-x-auto h-40 overflow-y-auto whitespace-pre-wrap">
                                            {res}
                                        </pre>
                                    </div>
                                })}
                            </div>

                            <div class="flex justify-end gap-2 border-t border-slate-100 pt-3">
                                <button
                                    class="px-4 py-2 border border-slate-200 hover:bg-slate-50 text-slate-600 text-sm font-semibold rounded-lg transition"
                                    on:click=move |_| {
                                        testing_version_id.set(None);
                                        test_result_output.set(None);
                                    }
                                >
                                    "Close"
                                </button>
                                <button
                                    class="px-4 py-2 bg-black hover:bg-zinc-900 text-white text-sm font-semibold rounded-lg transition shadow-sm"
                                    disabled=move || test_in_progress.get()
                                    on:click=move |_| {
                                        let v_id = v_id_c.clone();
                                        test_in_progress.set(true);
                                        test_result_output.set(None);
                                        spawn_local(async move {
                                            let raw_json = test_payload_input.get();
                                            let payload_val = match serde_json::from_str::<serde_json::Value>(&raw_json) {
                                                Ok(val) => val,
                                                Err(e) => {
                                                    test_result_output.set(Some(format!("Invalid Input JSON: {}", e)));
                                                    test_in_progress.set(false);
                                                    return;
                                                }
                                            };
                                            match api::test_version(&v_id, payload_val).await {
                                                Ok(res) => {
                                                    if let Some(status) = res.get("status").and_then(|s| s.as_str()) {
                                                        if status == "success" {
                                                            let logs = res.get("logs").and_then(|l| l.as_str()).unwrap_or("");
                                                            let output = res.get("output").map(|o| o.to_string()).unwrap_or_default();
                                                            test_result_output.set(Some(format!("--- SUCCESS ---\n\nSTDOUT/OUTPUT:\n{}\n\nCONTAINER LOGS:\n{}", output, logs)));
                                                        } else {
                                                            let err = res.get("error").and_then(|e| e.as_str()).unwrap_or("Unknown error");
                                                            test_result_output.set(Some(format!("--- FAILED ---\n\nERROR:\n{}", err)));
                                                        }
                                                    }
                                                }
                                                Err(e) => {
                                                    test_result_output.set(Some(format!("API call failed: {}", e)));
                                                }
                                            }
                                            test_in_progress.set(false);
                                        });
                                    }
                                >
                                    "Run Test"
                                </button>
                            </div>
                        </div>
                    </div>
                }
            })}
        </div>
    }
}

// =============================================================================
// SurrealDB Connection Pools CRUD View
// =============================================================================
#[component]
fn ConnectionsView() -> impl IntoView {
    let pools = RwSignal::new(Vec::<DbPoolDTO>::new());
    let conn_page = RwSignal::new(1);
    let show_only_active = RwSignal::new(true);
    let new_is_active = RwSignal::new(true);
    
    let new_name = RwSignal::new(String::new());
    let new_url = RwSignal::new(String::new());
    let new_ns = RwSignal::new(String::new());
    let new_db = RwSignal::new(String::new());
    let new_username = RwSignal::new(String::new());
    let new_password = RwSignal::new(String::new());
    let editing_pool_id = RwSignal::new(Option::<String>::None);

    let clear_form = move || {
        editing_pool_id.set(None);
        new_name.set(String::new());
        new_url.set(String::new());
        new_ns.set(String::new());
        new_db.set(String::new());
        new_username.set(String::new());
        new_password.set(String::new());
        new_is_active.set(true);
    };

    let reload = move || {
        spawn_local(async move {
            if let Ok(list) = api::get_pools().await {
                pools.set(list);
            }
        });
    };

    Effect::new(move |_| {
        reload();
    });

    view! {
        <div class="p-8 space-y-6">
            <div class="flex justify-between items-center">
                <h1 class="text-2xl font-bold text-slate-900">"SurrealDB Connection Pools"</h1>
                <label class="inline-flex items-center gap-2 text-sm font-semibold text-slate-600 bg-white px-3 py-1.5 border border-slate-200 rounded-lg shadow-sm cursor-pointer hover:bg-slate-50 transition">
                    <input
                        type="checkbox"
                        class="rounded border-slate-300 text-indigo-600 focus:ring-indigo-500 h-4 w-4"
                        prop:checked=show_only_active
                        on:change=move |ev| show_only_active.set(event_target_checked(&ev))
                    />
                    "Show Active Only"
                </label>
            </div>

            <div class="grid grid-cols-1 lg:grid-cols-3 gap-8">
                // Pools List
                <div class="lg:col-span-2">
                    <div class="bg-white border border-slate-200 rounded-xl overflow-hidden shadow-sm">
                        <table class="w-full text-left">
                            <thead class="bg-slate-50 border-b border-slate-200 text-xs font-semibold text-slate-500 uppercase">
                                <tr>
                                    <th class="px-6 py-3">"Pool Name"</th>
                                    <th class="px-6 py-3">"Connection URL"</th>
                                    <th class="px-6 py-3">"Target NS/DB"</th>
                                    <th class="px-6 py-3">"Actions"</th>
                                </tr>
                            </thead>
                            <tbody class="divide-y divide-slate-200 text-sm">
                                {move || {
                                    let raw_list = pools.get();
                                    let list = if show_only_active.get() {
                                        raw_list.into_iter().filter(|p| p.is_active).collect::<Vec<_>>()
                                    } else {
                                        raw_list
                                    };
                                    let total_len = list.len();
                                    let page = conn_page.get();
                                    let start = (page - 1) * 5;
                                    let end = std::cmp::min(page * 5, total_len);

                                    if total_len == 0 {
                                        return view! {
                                            <tr>
                                                <td colspan="4" class="px-6 py-8 text-center text-slate-400">
                                                    "No connection pools configured"
                                                </td>
                                            </tr>
                                        }.into_any();
                                    }

                                    let page_slice = list[start..end].to_vec();
                                    page_slice.into_iter().map(|pool| {
                                        let p_id = pool.id.clone().unwrap_or_default();
                                        let p_id_remove = p_id.clone();
                                        view! {
                                            <tr class="hover:bg-slate-50/50">
                                                <td class="px-6 py-4 font-semibold text-slate-900">
                                                    <div class="flex items-center gap-2">
                                                        <span>{pool.name.clone()}</span>
                                                        {if pool.is_active {
                                                            view! {
                                                                <span class="px-1.5 py-0.5 bg-green-50 text-green-700 rounded text-[9px] font-bold border border-green-200">
                                                                    "Active"
                                                                </span>
                                                            }.into_any()
                                                        } else {
                                                            view! {
                                                                <span class="px-1.5 py-0.5 bg-slate-100 text-slate-500 rounded text-[9px] font-bold border border-slate-300">
                                                                    "Inactive"
                                                                </span>
                                                            }.into_any()
                                                        }}
                                                    </div>
                                                </td>
                                                <td class="px-6 py-4 font-mono text-xs">{pool.connection_url.clone()}</td>
                                                <td class="px-6 py-4">
                                                    {format!("{}/{}", pool.auth_namespace.as_deref().unwrap_or_default(), pool.auth_database.as_deref().unwrap_or_default())}
                                                </td>
                                                <td class="px-6 py-4 space-x-2">
                                                    <button
                                                        class="text-indigo-600 hover:text-indigo-800 font-semibold"
                                                        on:click=move |_| {
                                                            let p_id = p_id.clone();
                                                            spawn_local(async move {
                                                                match api::test_pool_connection(&p_id).await {
                                                                    Ok(msg) => {
                                                                        if let Some(w) = web_sys::window() {
                                                                            let _ = w.alert_with_message(&format!("Connection OK!\n{}", msg));
                                                                        }
                                                                    }
                                                                    Err(err) => {
                                                                        if let Some(w) = web_sys::window() {
                                                                            let _ = w.alert_with_message(&format!("Connection Failed!\n{}", err));
                                                                        }
                                                                    }
                                                                }
                                                            });
                                                        }
                                                    >
                                                        "Test Connection"
                                                    </button>
                                                    <span class="text-slate-300">"|"</span>
                                                    <button
                                                        class="text-indigo-600 hover:text-indigo-800 font-semibold"
                                                        on:click=move |_| {
                                                            let p = pool.clone();
                                                            editing_pool_id.set(p.id.clone());
                                                            new_name.set(p.name.clone());
                                                            new_url.set(p.connection_url.clone());
                                                            new_ns.set(p.auth_namespace.unwrap_or_default());
                                                            new_db.set(p.auth_database.unwrap_or_default());
                                                            new_username.set(p.auth_username.unwrap_or_default());
                                                            new_password.set(p.auth_password.unwrap_or_default());
                                                            new_is_active.set(p.is_active);
                                                        }
                                                    >
                                                        "Edit"
                                                    </button>
                                                    <span class="text-slate-300">"|"</span>
                                                    <button
                                                        class="text-red-600 hover:text-red-800 font-semibold"
                                                        on:click=move |_| {
                                                            let p_id = p_id_remove.clone();
                                                            spawn_local(async move {
                                                                let _ = api::delete_pool(&p_id).await;
                                                                reload();
                                                            });
                                                        }
                                                    >
                                                        "Remove"
                                                    </button>
                                                </td>
                                            </tr>
                                        }
                                    }).collect::<Vec<_>>().into_any()
                                }}
                            </tbody>
                        </table>

                        // Pagination controls for connections
                        <div class="px-6 py-3 border-t border-slate-200 flex items-center justify-between bg-slate-50 text-xs font-semibold text-slate-500">
                            <div>
                                {move || {
                                    let raw_list = pools.get();
                                    let list = if show_only_active.get() {
                                        raw_list.into_iter().filter(|p| p.is_active).collect::<Vec<_>>()
                                    } else {
                                        raw_list
                                    };
                                    let total_len = list.len();
                                    let total_pages = (total_len + 5 - 1) / 5;
                                    format!("Page {} of {}", conn_page.get(), std::cmp::max(total_pages, 1))
                                }}
                            </div>
                            <div class="flex gap-2">
                                <button
                                    class="px-2 py-1 bg-white border border-slate-200 rounded text-slate-600 disabled:opacity-50 hover:bg-slate-50 transition"
                                    disabled=move || conn_page.get() <= 1
                                    on:click=move |_| conn_page.set(conn_page.get() - 1)
                                >
                                    "Previous"
                                </button>
                                <button
                                    class="px-2 py-1 bg-white border border-slate-200 rounded text-slate-600 disabled:opacity-50 hover:bg-slate-50 transition"
                                    disabled=move || {
                                        let raw_list = pools.get();
                                        let list = if show_only_active.get() {
                                            raw_list.into_iter().filter(|p| p.is_active).collect::<Vec<_>>()
                                        } else {
                                            raw_list
                                        };
                                        let total_len = list.len();
                                        let total_pages = if total_len == 0 { 1 } else { (total_len + 5 - 1) / 5 };
                                        conn_page.get() >= total_pages
                                    }
                                    on:click=move |_| conn_page.set(conn_page.get() + 1)
                                >
                                    "Next"
                                </button>
                            </div>
                        </div>
                    </div>
                </div>

                // Add Connection form
                <div class="bg-white border border-slate-200 rounded-xl p-6 space-y-4 h-fit shadow-sm">
                    <h3 class="font-bold text-slate-800 text-lg">"Add Database Pool"</h3>
                    
                    <div>
                        <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">"Name"</label>
                        <input
                            type="text"
                            placeholder="surreal-prod"
                            class="w-full bg-slate-50 border border-slate-300 rounded-lg px-3 py-2 text-slate-900 outline-none focus:border-indigo-500 text-sm"
                            prop:value=new_name
                            on:input=move |ev| new_name.set(event_target_value(&ev))
                        />
                    </div>

                    <div>
                        <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">"Connection URL"</label>
                        <input
                            type="text"
                            placeholder="ws://127.0.0.1:8000"
                            class="w-full bg-slate-50 border border-slate-300 rounded-lg px-3 py-2 text-slate-900 outline-none focus:border-indigo-500 text-sm"
                            prop:value=new_url
                            on:input=move |ev| new_url.set(event_target_value(&ev))
                        />
                    </div>

                    <div class="grid grid-cols-2 gap-4">
                        <div>
                            <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">"Namespace"</label>
                            <input
                                type="text"
                                placeholder="my-ns"
                                class="w-full bg-slate-50 border border-slate-300 rounded-lg px-3 py-2 text-slate-900 outline-none focus:border-indigo-500 text-sm"
                                prop:value=new_ns
                                on:input=move |ev| new_ns.set(event_target_value(&ev))
                            />
                        </div>
                        <div>
                            <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">"Database"</label>
                            <input
                                type="text"
                                placeholder="my-db"
                                class="w-full bg-slate-50 border border-slate-300 rounded-lg px-3 py-2 text-slate-900 outline-none focus:border-indigo-500 text-sm"
                                prop:value=new_db
                                on:input=move |ev| new_db.set(event_target_value(&ev))
                            />
                        </div>
                    </div>

                    <div class="grid grid-cols-2 gap-4">
                        <div>
                            <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">"Username"</label>
                            <input
                                type="text"
                                placeholder="root"
                                class="w-full bg-slate-50 border border-slate-300 rounded-lg px-3 py-2 text-slate-900 outline-none focus:border-indigo-500 text-sm"
                                prop:value=new_username
                                on:input=move |ev| new_username.set(event_target_value(&ev))
                            />
                        </div>
                        <div>
                            <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">"Password"</label>
                            <input
                                type="password"
                                placeholder="••••••••"
                                class="w-full bg-slate-50 border border-slate-300 rounded-lg px-3 py-2 text-slate-900 outline-none focus:border-indigo-500 text-sm"
                                prop:value=new_password
                                on:input=move |ev| new_password.set(event_target_value(&ev))
                            />
                        </div>
                    </div>

                    <div>
                        <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">"Active Status"</label>
                        <label class="inline-flex items-center gap-2 text-sm text-slate-700 cursor-pointer">
                            <input
                                type="checkbox"
                                class="rounded border-slate-300 text-indigo-600 focus:ring-indigo-500 h-4 w-4"
                                prop:checked=new_is_active
                                on:change=move |ev| new_is_active.set(event_target_checked(&ev))
                            />
                            "Connection Pool is Active"
                        </label>
                    </div>

                    <div class="flex flex-col gap-2">
                        <button
                            class="w-full bg-zinc-800 hover:bg-zinc-700 text-white font-semibold py-2 px-4 rounded-lg text-sm transition shadow-sm font-sans"
                            on:click=move |_| {
                                let connection_url = new_url.get();
                                if connection_url.is_empty() { return; }
                                spawn_local(async move {
                                    match api::test_pool_connection_payload(&connection_url).await {
                                        Ok(msg) => {
                                            if let Some(w) = web_sys::window() {
                                                let _ = w.alert_with_message(&format!("Connection OK!\n{}", msg));
                                            }
                                        }
                                        Err(err) => {
                                            if let Some(w) = web_sys::window() {
                                                let _ = w.alert_with_message(&format!("Connection Failed!\n{}", err));
                                            }
                                        }
                                    }
                                });
                            }
                        >
                            "Test Connection"
                        </button>

                        <button
                            class="w-full bg-black hover:bg-zinc-900 text-white font-semibold py-2 px-4 rounded-lg text-sm transition shadow-sm font-sans"
                            on:click=move |_| {
                                let name = new_name.get();
                                let connection_url = new_url.get();
                                let auth_namespace = Some(new_ns.get());
                                let auth_database = Some(new_db.get());
                                let auth_username = Some(new_username.get());
                                let auth_password = Some(new_password.get());
                                let active_status = new_is_active.get();
                                let edit_id = editing_pool_id.get();
                                
                                spawn_local(async move {
                                    let payload = DbPoolDTO {
                                        id: edit_id.clone(),
                                        name,
                                        engine: "surrealdb".to_string(),
                                        connection_url,
                                        auth_namespace,
                                        auth_database,
                                        auth_username,
                                        auth_password,
                                        max_connections: 10,
                                        tags: vec![],
                                        is_active: active_status,
                                        created_at: None,
                                    };
                                    
                                    let success = if let Some(ref id) = edit_id {
                                        api::update_pool(id, payload).await.is_ok()
                                    } else {
                                        api::create_pool(payload).await.is_ok()
                                    };

                                    if success {
                                        clear_form();
                                        reload();
                                    }
                                });
                            }
                        >
                            {move || if editing_pool_id.get().is_some() { "Save Changes" } else { "Add Connection Pool" }}
                        </button>

                        {move || if editing_pool_id.get().is_some() {
                            view! {
                                <button
                                    class="w-full bg-slate-100 hover:bg-slate-200 text-slate-700 font-semibold py-2 px-4 rounded-lg text-sm transition shadow-sm font-sans"
                                    on:click=move |_| clear_form()
                                >
                                    "Cancel"
                                </button>
                            }.into_any()
                        } else {
                            view! { <div /> }.into_any()
                        }}
                    </div>
                </div>
            </div>
        </div>
    }
}

// =============================================================================
// Redis Streams & Bindings View
// =============================================================================
#[component]
fn QueuesView() -> impl IntoView {
    let queues = RwSignal::new(Vec::<QueueDTO>::new());
    let queues_page = RwSignal::new(1);
    let new_stream_key = RwSignal::new(String::new());
    let show_only_active = RwSignal::new(true);
    let new_is_active = RwSignal::new(true);

    let reload = move || {
        spawn_local(async move {
            if let Ok(q_list) = api::get_queues().await {
                queues.set(q_list);
            }
        });
    };

    Effect::new(move |_| {
        reload();
    });

    view! {
        <div class="p-8 space-y-6">
            <div class="flex justify-between items-center">
                <h1 class="text-2xl font-bold text-slate-900">"Redis Streams / Queues"</h1>
                <label class="inline-flex items-center gap-2 text-sm font-semibold text-slate-600 bg-white px-3 py-1.5 border border-slate-200 rounded-lg shadow-sm cursor-pointer hover:bg-slate-50 transition">
                    <input
                        type="checkbox"
                        class="rounded border-slate-300 text-indigo-600 focus:ring-indigo-500 h-4 w-4"
                        prop:checked=show_only_active
                        on:change=move |ev| show_only_active.set(event_target_checked(&ev))
                    />
                    "Show Active Only"
                </label>
            </div>

            <div class="bg-white border border-slate-200 rounded-xl p-6 shadow-sm space-y-4">
                <h3 class="font-bold text-slate-800 text-lg">"Active Redis Streams"</h3>
                
                <div class="flex flex-col md:flex-row gap-4 max-w-2xl">
                    <input
                        type="text"
                        placeholder="orders_stream"
                        class="flex-1 bg-slate-50 border border-slate-300 rounded-lg px-3 py-2 text-slate-900 outline-none focus:border-indigo-500 text-sm"
                        on:input=move |ev| new_stream_key.set(event_target_value(&ev))
                        prop:value=new_stream_key
                    />
                    <label class="inline-flex items-center gap-2 text-sm text-slate-700 cursor-pointer">
                        <input
                            type="checkbox"
                            class="rounded border-slate-300 text-indigo-600 focus:ring-indigo-500 h-4 w-4"
                            prop:checked=new_is_active
                            on:change=move |ev| new_is_active.set(event_target_checked(&ev))
                        />
                        "Active"
                    </label>
                    <button
                        class="bg-black hover:bg-zinc-900 text-white font-semibold py-2 px-4 rounded-lg text-sm transition shadow-sm"
                        on:click=move |_| {
                            let key = new_stream_key.get();
                            if key.is_empty() { return; }
                            let active_status = new_is_active.get();
                            spawn_local(async move {
                                let payload = QueueDTO {
                                    id: None,
                                    stream_key: key,
                                    name: None,
                                    consumer_group: "orchestrator_group".to_string(),
                                    is_active: active_status,
                                    tags: vec![],
                                    created_at: None,
                                };
                                if let Ok(_) = api::create_queue(payload).await {
                                    new_stream_key.set(String::new());
                                    new_is_active.set(true);
                                    reload();
                                }
                            });
                        }
                    >
                        "Add Queue"
                    </button>
                </div>

                <div class="overflow-hidden border border-slate-200 rounded-xl">
                    <table class="w-full text-left">
                        <thead class="bg-slate-50 border-b border-slate-200 text-xs font-semibold text-slate-500 uppercase">
                            <tr>
                                <th class="px-6 py-3">"Stream Key"</th>
                                <th class="px-6 py-3">"Consumer Group"</th>
                                <th class="px-6 py-3">"Action"</th>
                            </tr>
                        </thead>
                        <tbody class="divide-y divide-slate-200 text-sm">
                            {move || {
                                let raw_list = queues.get();
                                let list = if show_only_active.get() {
                                    raw_list.into_iter().filter(|q| q.is_active).collect::<Vec<_>>()
                                } else {
                                    raw_list
                                };
                                let total_len = list.len();
                                let page = queues_page.get();
                                let start = (page - 1) * 5;
                                let end = std::cmp::min(page * 5, total_len);

                                if total_len == 0 {
                                    return view! {
                                        <tr>
                                            <td colspan="3" class="px-6 py-8 text-center text-slate-400">
                                                "No active queues configured"
                                            </td>
                                        </tr>
                                    }.into_any();
                                }

                                let page_slice = list[start..end].to_vec();
                                page_slice.into_iter().map(|queue| {
                                    let q_id = queue.id.clone().unwrap_or_default();
                                    view! {
                                        <tr class="hover:bg-slate-50/50">
                                            <td class="px-6 py-4 font-mono font-medium text-slate-900">
                                                <div class="flex items-center gap-2">
                                                    <span>{queue.stream_key}</span>
                                                    {if queue.is_active {
                                                        view! {
                                                            <span class="px-1.5 py-0.5 bg-green-50 text-green-700 rounded text-[9px] font-bold border border-green-200">
                                                                "Active"
                                                            </span>
                                                        }.into_any()
                                                    } else {
                                                        view! {
                                                            <span class="px-1.5 py-0.5 bg-slate-100 text-slate-500 rounded text-[9px] font-bold border border-slate-300">
                                                                "Inactive"
                                                            </span>
                                                        }.into_any()
                                                    }}
                                                </div>
                                            </td>
                                            <td class="px-6 py-4 text-slate-500">{queue.consumer_group}</td>
                                            <td class="px-6 py-4">
                                                <button
                                                    class="text-red-600 hover:text-red-800 font-semibold"
                                                    on:click=move |_| {
                                                        let q_id = q_id.clone();
                                                        spawn_local(async move {
                                                            match api::delete_queue(&q_id).await {
                                                                Ok(_) => reload(),
                                                                Err(e) => {
                                                                    let window = web_sys::window().unwrap();
                                                                    let _ = window.alert_with_message(&e.to_string());
                                                                }
                                                            }
                                                        });
                                                    }
                                                >
                                                    "Remove"
                                                </button>
                                            </td>
                                        </tr>
                                    }
                                }).collect::<Vec<_>>().into_any()
                            }}
                        </tbody>
                    </table>

                    // Pagination controls for queues
                    <div class="px-6 py-3 border-t border-slate-200 flex items-center justify-between bg-slate-50 text-xs font-semibold text-slate-500">
                        <div>
                            {move || {
                                let total_len = queues.get().len();
                                let total_pages = (total_len + 5 - 1) / 5;
                                format!("Page {} of {}", queues_page.get(), std::cmp::max(total_pages, 1))
                            }}
                        </div>
                        <div class="flex gap-2">
                            <button
                                class="px-2 py-1 bg-white border border-slate-200 rounded text-slate-600 disabled:opacity-50 hover:bg-slate-50 transition"
                                disabled=move || queues_page.get() <= 1
                                on:click=move |_| queues_page.set(queues_page.get() - 1)
                            >
                                "Previous"
                            </button>
                            <button
                                class="px-2 py-1 bg-white border border-slate-200 rounded text-slate-600 disabled:opacity-50 hover:bg-slate-50 transition"
                                disabled=move || {
                                    let total_len = queues.get().len();
                                    let total_pages = if total_len == 0 { 1 } else { (total_len + 5 - 1) / 5 };
                                    queues_page.get() >= total_pages
                                }
                                on:click=move |_| queues_page.set(queues_page.get() + 1)
                            >
                                "Next"
                            </button>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    }
}

#[component]
fn BindingsView() -> impl IntoView {
    let queues = RwSignal::new(Vec::<QueueDTO>::new());
    let bindings = RwSignal::new(Vec::<BindingDTO>::new());
    let microservices = RwSignal::new(Vec::<MicroserviceDTO>::new());

    // Binding form state
    let bind_queue_id = RwSignal::new(String::new());
    let bind_service_id = RwSignal::new(String::new());
    let success_action = RwSignal::new("ack".to_string());
    let success_config = RwSignal::new(String::new());
    let error_action = RwSignal::new("ack".to_string());
    let error_config = RwSignal::new(String::new());

    // Success Key Event signals
    let success_ke_key = RwSignal::new(String::new());
    let success_ke_op = RwSignal::new("==".to_string());
    let success_ke_val = RwSignal::new(String::new());
    let success_ke_stream = RwSignal::new(String::new());

    // Error Key Event signals
    let error_ke_key = RwSignal::new(String::new());
    let error_ke_op = RwSignal::new("==".to_string());
    let error_ke_val = RwSignal::new(String::new());
    let error_ke_stream = RwSignal::new(String::new());

    let reload = move || {
        spawn_local(async move {
            if let Ok(q_list) = api::get_queues().await {
                queues.set(q_list);
            }
            if let Ok(b_list) = api::get_bindings().await {
                bindings.set(b_list);
            }
            if let Ok(m_list) = api::get_services().await {
                microservices.set(m_list);
            }
        });
    };

    Effect::new(move |_| {
        reload();
    });

    view! {
        <div class="p-8 space-y-6">
            <h1 class="text-2xl font-bold text-slate-900">"Event Bindings CRUD"</h1>

            <div class="grid grid-cols-1 lg:grid-cols-3 gap-8">
                // Left: Create Binding Form (span 1)
                <div class="bg-white border border-slate-200 rounded-xl p-6 shadow-sm space-y-4 h-fit">
                    <h3 class="font-bold text-slate-800 text-lg">"New Event Binding"</h3>
                    
                    <div class="space-y-4">
                        <div>
                            <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">"Select Stream Key"</label>
                            <select
                                class="w-full bg-slate-50 border border-slate-300 rounded-lg px-3 py-2 text-slate-900 outline-none focus:border-indigo-500 text-sm"
                                on:change=move |ev| bind_queue_id.set(event_target_value(&ev))
                            >
                                <option value="">"Select..."</option>
                                {move || queues.get().into_iter().map(|queue| view! {
                                    <option value=queue.id.clone().unwrap_or_default()>{queue.stream_key}</option>
                                }).collect::<Vec<_>>()}
                            </select>
                        </div>

                        <div>
                            <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">"Select Microservice"</label>
                            <select
                                class="w-full bg-slate-50 border border-slate-300 rounded-lg px-3 py-2 text-slate-900 outline-none focus:border-indigo-500 text-sm"
                                on:change=move |ev| bind_service_id.set(event_target_value(&ev))
                            >
                                <option value="">"Select..."</option>
                                {move || microservices.get().into_iter().map(|ms| view! {
                                    <option value=ms.id.clone().unwrap_or_default()>{ms.name}</option>
                                }).collect::<Vec<_>>()}
                            </select>
                        </div>
                    </div>

                    <div class="space-y-4 border-t border-slate-100 pt-4">
                        <div>
                            <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">"On Success Action"</label>
                            <select
                                class="w-full bg-slate-50 border border-slate-300 rounded-lg px-3 py-2 text-slate-900 outline-none focus:border-indigo-500 text-sm"
                                on:change=move |ev| success_action.set(event_target_value(&ev))
                            >
                                <option value="ack">"Acknowledge Only (Finalize)"</option>
                                <option value="publish">"Publish to another Stream"</option>
                                <option value="key_event">"Key Event Condition"</option>
                            </select>
                        </div>

                        {move || if success_action.get() == "publish" {
                            view! {
                                <div>
                                    <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">"Success Destination Stream"</label>
                                    <select
                                        class="w-full bg-slate-50 border border-slate-300 rounded-lg px-3 py-2 text-slate-900 outline-none focus:border-indigo-500 text-sm"
                                        on:change=move |ev| success_config.set(event_target_value(&ev))
                                    >
                                        <option value="">"Select..."</option>
                                        {move || queues.get().into_iter().map(|queue| {
                                            let s_key = queue.stream_key.clone();
                                            view! {
                                                <option value=s_key.clone()>{s_key.clone()}</option>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </select>
                                </div>
                            }.into_any()
                        } else if success_action.get() == "key_event" {
                            view! {
                                <div class="bg-indigo-50/50 p-4 border border-indigo-100 rounded-xl space-y-3 mt-2">
                                    <div class="grid grid-cols-1 gap-2">
                                        <div>
                                            <label class="block text-[10px] font-semibold text-slate-500 uppercase tracking-wider mb-1">"Key Path (e.g. status)"</label>
                                            <input
                                                type="text"
                                                placeholder="status"
                                                class="w-full bg-white border border-slate-300 rounded-lg px-2 py-1 text-slate-900 outline-none text-xs"
                                                on:input=move |ev| success_ke_key.set(event_target_value(&ev))
                                            />
                                        </div>
                                        <div>
                                            <label class="block text-[10px] font-semibold text-slate-500 uppercase tracking-wider mb-1">"Operator"</label>
                                            <select
                                                class="w-full bg-white border border-slate-300 rounded-lg px-2 py-1 text-slate-900 outline-none text-xs"
                                                on:change=move |ev| success_ke_op.set(event_target_value(&ev))
                                            >
                                                <option value="==">"=="</option>
                                                <option value="!=">"!="</option>
                                                <option value=">">">"</option>
                                                <option value="<">"<"</option>
                                            </select>
                                        </div>
                                        <div>
                                            <label class="block text-[10px] font-semibold text-slate-500 uppercase tracking-wider mb-1">"Value to Compare"</label>
                                            <input
                                                type="text"
                                                placeholder="success"
                                                class="w-full bg-white border border-slate-300 rounded-lg px-2 py-1 text-slate-900 outline-none text-xs"
                                                on:input=move |ev| success_ke_val.set(event_target_value(&ev))
                                            />
                                        </div>
                                        <div>
                                            <label class="block text-[10px] font-semibold text-slate-500 uppercase tracking-wider mb-1">"Destination Stream"</label>
                                            <select
                                                class="w-full bg-white border border-slate-300 rounded-lg px-2 py-1 text-slate-900 outline-none text-xs"
                                                on:change=move |ev| success_ke_stream.set(event_target_value(&ev))
                                            >
                                                <option value="">"Select..."</option>
                                                {move || queues.get().into_iter().map(|queue| {
                                                    let s_key = queue.stream_key.clone();
                                                    view! {
                                                        <option value=s_key.clone()>{s_key.clone()}</option>
                                                    }
                                                }).collect::<Vec<_>>()}
                                            </select>
                                        </div>
                                    </div>
                                </div>
                            }.into_any()
                        } else {
                            view! { <div /> }.into_any()
                        }}
                    </div>

                    <div class="space-y-4 border-t border-slate-100 pt-4">
                        <div>
                            <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">"On Error Action"</label>
                            <select
                                class="w-full bg-slate-50 border border-slate-300 rounded-lg px-3 py-2 text-slate-900 outline-none focus:border-indigo-500 text-sm"
                                on:change=move |ev| error_action.set(event_target_value(&ev))
                            >
                                <option value="ack">"Acknowledge Only (Finalize)"</option>
                                <option value="publish">"Publish to another Stream"</option>
                                <option value="key_event">"Key Event Condition"</option>
                            </select>
                        </div>

                        {move || if error_action.get() == "publish" {
                            view! {
                                <div>
                                    <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">"Error Destination Stream"</label>
                                    <select
                                        class="w-full bg-slate-50 border border-slate-300 rounded-lg px-3 py-2 text-slate-900 outline-none focus:border-indigo-500 text-sm"
                                        on:change=move |ev| error_config.set(event_target_value(&ev))
                                    >
                                        <option value="">"Select..."</option>
                                        {move || queues.get().into_iter().map(|queue| {
                                            let s_key = queue.stream_key.clone();
                                            view! {
                                                <option value=s_key.clone()>{s_key.clone()}</option>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </select>
                                </div>
                            }.into_any()
                        } else if error_action.get() == "key_event" {
                            view! {
                                <div class="bg-indigo-50/50 p-4 border border-indigo-100 rounded-xl space-y-3 mt-2">
                                    <div class="grid grid-cols-1 gap-2">
                                        <div>
                                            <label class="block text-[10px] font-semibold text-slate-500 uppercase tracking-wider mb-1">"Key Path (e.g. status)"</label>
                                            <input
                                                type="text"
                                                placeholder="status"
                                                class="w-full bg-white border border-slate-300 rounded-lg px-2 py-1 text-slate-900 outline-none text-xs"
                                                on:input=move |ev| error_ke_key.set(event_target_value(&ev))
                                            />
                                        </div>
                                        <div>
                                            <label class="block text-[10px] font-semibold text-slate-500 uppercase tracking-wider mb-1">"Operator"</label>
                                            <select
                                                class="w-full bg-white border border-slate-300 rounded-lg px-2 py-1 text-slate-900 outline-none text-xs"
                                                on:change=move |ev| error_ke_op.set(event_target_value(&ev))
                                            >
                                                <option value="==">"=="</option>
                                                <option value="!=">"!="</option>
                                                <option value=">">">"</option>
                                                <option value="<">"<"</option>
                                            </select>
                                        </div>
                                        <div>
                                            <label class="block text-[10px] font-semibold text-slate-500 uppercase tracking-wider mb-1">"Value to Compare"</label>
                                            <input
                                                type="text"
                                                placeholder="success"
                                                class="w-full bg-white border border-slate-300 rounded-lg px-2 py-1 text-slate-900 outline-none text-xs"
                                                on:input=move |ev| error_ke_val.set(event_target_value(&ev))
                                            />
                                        </div>
                                        <div>
                                            <label class="block text-[10px] font-semibold text-slate-500 uppercase tracking-wider mb-1">"Destination Stream"</label>
                                            <select
                                                class="w-full bg-white border border-slate-300 rounded-lg px-2 py-1 text-slate-900 outline-none text-xs"
                                                on:change=move |ev| error_ke_stream.set(event_target_value(&ev))
                                            >
                                                <option value="">"Select..."</option>
                                                {move || queues.get().into_iter().map(|queue| {
                                                    let s_key = queue.stream_key.clone();
                                                    view! {
                                                        <option value=s_key.clone()>{s_key.clone()}</option>
                                                    }
                                                }).collect::<Vec<_>>()}
                                            </select>
                                        </div>
                                    </div>
                                </div>
                            }.into_any()
                        } else {
                            view! { <div /> }.into_any()
                        }}
                    </div>

                    <button
                        class="w-full bg-black hover:bg-zinc-900 text-white font-semibold py-2 px-4 rounded-lg text-sm transition shadow-sm mt-4"
                        on:click=move |_| {
                            let queue_id = bind_queue_id.get();
                            let microservice_id = bind_service_id.get();
                            if queue_id.is_empty() || microservice_id.is_empty() { return; }
                            
                            let s_action = success_action.get();
                            let s_config = if s_action == "publish" {
                                serde_json::Value::String(success_config.get())
                            } else if s_action == "key_event" {
                                serde_json::json!({
                                    "key": success_ke_key.get(),
                                    "operator": success_ke_op.get(),
                                    "value": success_ke_val.get(),
                                    "target_stream": success_ke_stream.get()
                                })
                            } else {
                                serde_json::Value::Null
                            };
                            let e_action = error_action.get();
                            let e_config = if e_action == "publish" {
                                serde_json::Value::String(error_config.get())
                            } else if e_action == "key_event" {
                                serde_json::json!({
                                    "key": error_ke_key.get(),
                                    "operator": error_ke_op.get(),
                                    "value": error_ke_val.get(),
                                    "target_stream": error_ke_stream.get()
                                })
                            } else {
                                serde_json::Value::Null
                            };
                            
                            spawn_local(async move {
                                let payload = BindingDTO {
                                    id: None,
                                    queue_id,
                                    microservice_id,
                                    queue: None,
                                    microservice: None,
                                    target_version_id: None,
                                    on_success_action: s_action,
                                    on_success_config: s_config,
                                    on_error_action: e_action,
                                    on_error_config: e_config,
                                    is_active: true,
                                };
                                if let Ok(_) = api::create_binding(payload).await {
                                    reload();
                                }
                            });
                        }
                    >
                        "Bind Event trigger"
                    </button>
                </div>

                // Right: Active Bindings List (span 2)
                <div class="lg:col-span-2 bg-white border border-slate-200 rounded-xl p-6 shadow-sm space-y-4">
                    <h3 class="font-bold text-slate-800 text-lg">"Active Event Bindings"</h3>
                    
                    <div class="overflow-hidden border border-slate-200 rounded-xl">
                        <table class="w-full text-left">
                            <thead class="bg-slate-50 border-b border-slate-200 text-xs font-semibold text-slate-500 uppercase">
                                <tr>
                                    <th class="px-6 py-3">"Stream Key / Service"</th>
                                    <th class="px-6 py-3">"Success Route"</th>
                                    <th class="px-6 py-3">"Error Route"</th>
                                    <th class="px-6 py-3 text-right">"Actions"</th>
                                </tr>
                            </thead>
                            <tbody class="divide-y divide-slate-200 text-sm">
                                {move || {
                                    let list = bindings.get();
                                    if list.is_empty() {
                                        view! {
                                            <tr>
                                                <td colspan="4" class="px-6 py-8 text-center text-slate-400">
                                                    "No active event bindings configured"
                                                </td>
                                            </tr>
                                        }.into_any()
                                    } else {
                                        list.into_iter().map(|bind| {
                                            let b_id = bind.id.clone().unwrap_or_default();
                                            let local_queue = queues.get().into_iter().find(|q| q.id.as_ref() == Some(&bind.queue_id));
                                            let local_ms = microservices.get().into_iter().find(|m| m.id.as_ref() == Some(&bind.microservice_id));

                                            let q_name = bind.queue.as_ref()
                                                .and_then(|q| q.name.clone().or_else(|| Some(q.stream_key.clone())))
                                                .or_else(|| local_queue.and_then(|q| q.name.clone().or_else(|| Some(q.stream_key.clone()))))
                                                .unwrap_or_else(|| format!("Unknown ({})", bind.queue_id));
                                            let ms_name = bind.microservice.as_ref()
                                                .map(|m| m.name.clone())
                                                .or_else(|| local_ms.map(|m| m.name.clone()))
                                                .unwrap_or_else(|| format!("Unknown ({})", bind.microservice_id));
                                            
                                            let s_route = match bind.on_success_action.as_str() {
                                                "publish" => format!("Publish: {}", bind.on_success_config.as_str().unwrap_or("")),
                                                "key_event" => "Key Event Routing".to_string(),
                                                _ => "Acknowledge Only".to_string()
                                            };
                                            let e_route = match bind.on_error_action.as_str() {
                                                "publish" => format!("Publish: {}", bind.on_error_config.as_str().unwrap_or("")),
                                                "key_event" => "Key Event Routing".to_string(),
                                                _ => "Acknowledge Only".to_string()
                                            };

                                            view! {
                                                <tr class="hover:bg-slate-50/50">
                                                    <td class="px-6 py-4">
                                                        <span class="text-slate-900 block font-bold">{q_name}</span>
                                                        <span class="text-slate-500 text-xs font-mono">{format!("↳ Microservice: {}", ms_name)}</span>
                                                    </td>
                                                    <td class="px-6 py-4 text-xs text-slate-600 font-semibold">{s_route}</td>
                                                    <td class="px-6 py-4 text-xs text-slate-600 font-semibold">{e_route}</td>
                                                    <td class="px-6 py-4 text-right">
                                                        <button
                                                            class="text-red-600 hover:text-red-800 font-semibold"
                                                            on:click=move |_| {
                                                                let b_id = b_id.clone();
                                                                spawn_local(async move {
                                                                    let _ = api::delete_binding(&b_id).await;
                                                                    reload();
                                                                });
                                                            }
                                                        >
                                                            "Unbind"
                                                        </button>
                                                    </td>
                                                </tr>
                                            }
                                        }).collect::<Vec<_>>().into_any()
                                    }
                                }}
                            </tbody>
                        </table>
                    </div>
                </div>
            </div>
        </div>
    }
}

// =============================================================================
// Advanced Execution Logs view
// =============================================================================
fn format_relative_time(dt: Option<chrono::DateTime<chrono::Utc>>) -> String {
    let dt = match dt {
        Some(t) => t,
        None => return "Unknown".to_string(),
    };
    // Note: since JS and WASM execution might differ, we fetch the current time as Utc::now()
    let now = chrono::Utc::now();
    let diff = now.signed_duration_since(dt);
    let seconds = diff.num_seconds();

    if seconds < 0 {
        return "just now".to_string();
    }
    if seconds < 60 {
        return format!("{}s ago", seconds);
    }
    let minutes = diff.num_minutes();
    if minutes < 60 {
        return format!("{}m ago", minutes);
    }
    let hours = diff.num_hours();
    if hours < 24 {
        return format!("{}h ago", hours);
    }
    let days = diff.num_days();
    if days < 30 {
        if days == 1 {
            return "yesterday".to_string();
        }
        return format!("{}d ago", days);
    }
    let months = days / 30;
    if months < 12 {
        if months == 1 {
            return "1 month ago".to_string();
        }
        return format!("{} months ago", months);
    }
    let years = days / 365;
    if years == 1 {
        return "1 year ago".to_string();
    }
    format!("{} years ago", years)
}

fn parse_datetime_input(s: &str, is_end_of_day: bool) -> Option<chrono::DateTime<chrono::Utc>> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M") {
        return Some(chrono::DateTime::from_naive_utc_and_offset(dt, chrono::Utc));
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Some(chrono::DateTime::from_naive_utc_and_offset(dt, chrono::Utc));
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let t = if is_end_of_day {
            chrono::NaiveTime::from_hms_opt(23, 59, 59)?
        } else {
            chrono::NaiveTime::from_hms_opt(0, 0, 0)?
        };
        let dt = chrono::NaiveDateTime::new(d, t);
        return Some(chrono::DateTime::from_naive_utc_and_offset(dt, chrono::Utc));
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&chrono::Utc));
    }
    None
}

#[component]
fn LogsView() -> impl IntoView {
    let logs = RwSignal::new(Vec::<ExecutionLogDTO>::new());
    let logs_page = RwSignal::new(1);
    let filter_status = RwSignal::new(None::<String>);
    let filter_microservice = RwSignal::new(None::<String>);
    let filter_queue = RwSignal::new(None::<String>);
    let filter_tag = RwSignal::new(String::new());
    let filter_start_date = RwSignal::new(String::new());
    let filter_end_date = RwSignal::new(String::new());
    let search_term = RwSignal::new(String::new());
    let filter_collapsed = RwSignal::new(false);
    let selected_log = RwSignal::new(None::<ExecutionLogDTO>);

    // Listen to close_all_modals event (dispatched on ESC keypress)
    if let Some(window) = web_sys::window() {
        let cb = Closure::<dyn FnMut(web_sys::CustomEvent)>::new(move |_ev: web_sys::CustomEvent| {
            selected_log.set(None);
        });
        let _ = window.add_event_listener_with_callback("close_all_modals", cb.as_ref().unchecked_ref());
        cb.forget();
    }

    let queues = RwSignal::new(Vec::<QueueDTO>::new());
    let microservices = RwSignal::new(Vec::<MicroserviceDTO>::new());

    let reload = move || {
        let status = filter_status.get();
        let ms_id = filter_microservice.get();
        let q_id = filter_queue.get();
        let tag_val = filter_tag.get();
        let start_val = filter_start_date.get();
        let end_val = filter_end_date.get();
        let term = search_term.get();

        spawn_local(async move {
            if let Ok(q_list) = api::get_queues().await {
                queues.set(q_list);
            }
            if let Ok(m_list) = api::get_services().await {
                microservices.set(m_list);
            }

            let start_dt = parse_datetime_input(&start_val, false);
            let end_dt = parse_datetime_input(&end_val, true);
            let tags_vec = if tag_val.trim().is_empty() {
                None
            } else {
                Some(vec![tag_val.trim().to_string()])
            };

            let query = LogFilterQuery {
                microservice_id: ms_id,
                queue_id: q_id,
                status,
                tags: tags_vec,
                start_date: start_dt,
                end_date: end_dt,
                min_duration_ms: None,
                max_duration_ms: None,
                search_term: if term.trim().is_empty() { None } else { Some(term) },
                page: 1,
                limit: 100,
            };
            if let Ok(res) = api::search_logs(query).await {
                logs.set(res.logs);
            }
        });
    };

    let reset_filters = move || {
        filter_status.set(None);
        filter_microservice.set(None);
        filter_queue.set(None);
        filter_tag.set(String::new());
        filter_start_date.set(String::new());
        filter_end_date.set(String::new());
        search_term.set(String::new());
        logs_page.set(1);
        reload();
    };

    let active_filter_count = move || {
        let mut c = 0;
        if filter_status.get().is_some() { c += 1; }
        if filter_microservice.get().is_some() { c += 1; }
        if filter_queue.get().is_some() { c += 1; }
        if !filter_tag.get().trim().is_empty() { c += 1; }
        if !filter_start_date.get().trim().is_empty() { c += 1; }
        if !filter_end_date.get().trim().is_empty() { c += 1; }
        if !search_term.get().trim().is_empty() { c += 1; }
        c
    };

    let apply_preset_1h = move || {
        let now = chrono::Utc::now();
        let one_hour_ago = now - chrono::Duration::hours(1);
        filter_start_date.set(one_hour_ago.format("%Y-%m-%dT%H:%M").to_string());
        filter_end_date.set(now.format("%Y-%m-%dT%H:%M").to_string());
        logs_page.set(1);
        reload();
    };

    let apply_preset_today = move || {
        let now = chrono::Utc::now().date_naive();
        let start = chrono::NaiveDateTime::new(now, chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap());
        let end = chrono::NaiveDateTime::new(now, chrono::NaiveTime::from_hms_opt(23, 59, 59).unwrap());
        filter_start_date.set(start.format("%Y-%m-%dT%H:%M").to_string());
        filter_end_date.set(end.format("%Y-%m-%dT%H:%M").to_string());
        logs_page.set(1);
        reload();
    };

    let apply_preset_7d = move || {
        let now = chrono::Utc::now();
        let seven_days_ago = now - chrono::Duration::days(7);
        filter_start_date.set(seven_days_ago.format("%Y-%m-%dT%H:%M").to_string());
        filter_end_date.set(now.format("%Y-%m-%dT%H:%M").to_string());
        logs_page.set(1);
        reload();
    };

    Effect::new(move |_| {
        reload();
    });

    view! {
        <div class="p-8 space-y-6 max-w-7xl mx-auto">
            // Header
            <div class="flex flex-col md:flex-row md:items-center md:justify-between gap-4">
                <div>
                    <h1 class="text-2xl font-extrabold text-slate-900 tracking-tight">"Execution Logs Manager"</h1>
                    <p class="text-sm text-slate-500 mt-1">"Audit, filter, and inspect microservice executions across streams, status, tags, and date ranges."</p>
                </div>
                
                <div class="flex items-center gap-2">
                    <button
                        class="bg-white hover:bg-slate-50 border border-slate-200 text-slate-700 px-4 py-2 rounded-lg text-sm transition font-medium shadow-sm flex items-center gap-1.5 active:scale-95"
                        on:click=move |_| reload()
                    >
                        <svg class="w-4 h-4 text-slate-500" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M4 4v5h.582m15.356 2A8.001 8.001 0 1121.21 7.89M9 11l3-3m0 0l3 3m-3-3v12" />
                        </svg>
                        "Refresh"
                    </button>
                    <button
                        class="bg-white hover:bg-slate-50 border border-slate-200 text-slate-700 px-4 py-2 rounded-lg text-sm transition font-medium shadow-sm flex items-center gap-1.5 active:scale-95"
                        on:click=move |_| filter_collapsed.set(!filter_collapsed.get())
                    >
                        <svg class="w-4 h-4 text-slate-500" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 4a1 1 0 011-1h16a1 1 0 011 1v2.586a1 1 0 01-.293.707l-6.414 6.414a1 1 0 00-.293.707V17l-4 4v-6.586a1 1 0 00-.293-.707L3.293 7.293A1 1 0 013 6.586V4z" />
                        </svg>
                        <span>{move || if filter_collapsed.get() { "Mostrar Filtros" } else { "Ocultar Filtros" }}</span>
                        {move || {
                            let c = active_filter_count();
                            if c > 0 {
                                view! {
                                    <span class="ml-1 px-1.5 py-0.5 rounded-full text-[10px] font-bold bg-slate-900 text-white">
                                        {c}
                                    </span>
                                }.into_any()
                            } else {
                                view! { <div></div> }.into_any()
                            }
                        }}
                    </button>
                </div>
            </div>

            // Filter Panel
            <div class=move || format!(
                "bg-white border border-slate-200 rounded-xl overflow-hidden transition-all duration-300 shadow-sm {}",
                if filter_collapsed.get() { "max-h-0 border-none p-0 opacity-0 hidden" } else { "p-6 opacity-100 block" }
            )>
                <div class="space-y-4">
                    // Row 1: Selectors & Inputs
                    <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
                        // 1. Microsserviço
                        <div>
                            <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-1.5">"Microsserviço"</label>
                            <select
                                class="w-full bg-slate-50 border border-slate-200 rounded-lg px-3 py-2 text-slate-900 outline-none focus:border-black text-sm"
                                prop:value=move || filter_microservice.get().unwrap_or_else(|| "all".to_string())
                                on:change=move |ev| {
                                    let val = event_target_value(&ev);
                                    filter_microservice.set(if val == "all" { None } else { Some(val) });
                                    logs_page.set(1);
                                    reload();
                                }
                            >
                                <option value="all">"Todos os Microsserviços"</option>
                                {move || microservices.get().into_iter().map(|m| {
                                    let m_id = m.id.clone().unwrap_or_default();
                                    let m_label = format!("{} (#{})", m.name, m_id);
                                    view! {
                                        <option value=m_id>{m_label}</option>
                                    }
                                }).collect::<Vec<_>>()}
                            </select>
                        </div>

                        // 2. Queue / Stream
                        <div>
                            <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-1.5">"Queue (Stream)"</label>
                            <select
                                class="w-full bg-slate-50 border border-slate-200 rounded-lg px-3 py-2 text-slate-900 outline-none focus:border-black text-sm"
                                prop:value=move || filter_queue.get().unwrap_or_else(|| "all".to_string())
                                on:change=move |ev| {
                                    let val = event_target_value(&ev);
                                    filter_queue.set(if val == "all" { None } else { Some(val) });
                                    logs_page.set(1);
                                    reload();
                                }
                            >
                                <option value="all">"Todas as Queues"</option>
                                {move || queues.get().into_iter().map(|q| {
                                    let q_id = q.id.clone().unwrap_or_default();
                                    let q_label = format!("{} (#{})", q.stream_key, q_id);
                                    view! {
                                        <option value=q_id>{q_label}</option>
                                    }
                                }).collect::<Vec<_>>()}
                            </select>
                        </div>

                        // 3. Status
                        <div>
                            <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-1.5">"Status"</label>
                            <select
                                class="w-full bg-slate-50 border border-slate-200 rounded-lg px-3 py-2 text-slate-900 outline-none focus:border-black text-sm"
                                prop:value=move || filter_status.get().unwrap_or_else(|| "all".to_string())
                                on:change=move |ev| {
                                    let val = event_target_value(&ev);
                                    filter_status.set(if val == "all" { None } else { Some(val) });
                                    logs_page.set(1);
                                    reload();
                                }
                            >
                                <option value="all">"Todos os Status"</option>
                                <option value="success">"Success"</option>
                                <option value="error">"Error"</option>
                                <option value="timeout">"Timeout"</option>
                                <option value="panic">"Panic"</option>
                            </select>
                        </div>

                        // 4. Tag
                        <div>
                            <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-1.5">"Tag"</label>
                            <input
                                type="text"
                                placeholder="Ex: auth, billing, v1..."
                                class="w-full bg-slate-50 border border-slate-200 rounded-lg px-3 py-2 text-slate-900 outline-none focus:border-black text-sm"
                                prop:value=move || filter_tag.get()
                                on:input=move |ev| {
                                    filter_tag.set(event_target_value(&ev));
                                    logs_page.set(1);
                                    reload();
                                }
                            />
                        </div>
                    </div>

                    // Row 2: Date Ranges & Search
                    <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 pt-3 border-t border-slate-100">
                        // 5. Data Inicial (Start Date Range)
                        <div>
                            <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-1.5">"Data Inicial (De)"</label>
                            <input
                                type="datetime-local"
                                class="w-full bg-slate-50 border border-slate-200 rounded-lg px-3 py-2 text-slate-900 outline-none focus:border-black text-sm font-mono"
                                prop:value=move || filter_start_date.get()
                                on:input=move |ev| {
                                    filter_start_date.set(event_target_value(&ev));
                                    logs_page.set(1);
                                    reload();
                                }
                            />
                        </div>

                        // 6. Data Final (End Date Range)
                        <div>
                            <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-1.5">"Data Final (Até)"</label>
                            <input
                                type="datetime-local"
                                class="w-full bg-slate-50 border border-slate-200 rounded-lg px-3 py-2 text-slate-900 outline-none focus:border-black text-sm font-mono"
                                prop:value=move || filter_end_date.get()
                                on:input=move |ev| {
                                    filter_end_date.set(event_target_value(&ev));
                                    logs_page.set(1);
                                    reload();
                                }
                            />
                        </div>

                        // 7. Busca Textual (Search Term)
                        <div>
                            <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-1.5">"Busca Textual (Erro / Payload)"</label>
                            <input
                                type="text"
                                placeholder="Ex: connection refused, User..."
                                class="w-full bg-slate-50 border border-slate-200 rounded-lg px-3 py-2 text-slate-900 outline-none focus:border-black text-sm"
                                prop:value=move || search_term.get()
                                on:input=move |ev| {
                                    search_term.set(event_target_value(&ev));
                                    logs_page.set(1);
                                    reload();
                                }
                            />
                        </div>

                        // 8. Botões de Ação
                        <div class="flex items-end gap-2">
                            <button
                                class="flex-1 py-2 px-3 bg-slate-900 hover:bg-black text-white text-sm font-semibold rounded-lg shadow-sm transition text-center active:scale-95"
                                on:click=move |_| {
                                    logs_page.set(1);
                                    reload();
                                }
                            >
                                "Aplicar"
                            </button>
                            <button
                                class="py-2 px-3 bg-slate-100 hover:bg-slate-200 text-slate-700 text-sm font-medium rounded-lg transition text-center active:scale-95"
                                on:click=move |_| reset_filters()
                            >
                                "Limpar"
                            </button>
                        </div>
                    </div>

                    // Quick Date Presets Row
                    <div class="flex flex-wrap items-center gap-2 pt-2 text-xs">
                        <span class="text-slate-400 font-medium">"Atalhos de data:"</span>
                        <button
                            class="px-2.5 py-1 rounded bg-slate-100 hover:bg-slate-200 text-slate-700 font-medium transition"
                            on:click=move |_| apply_preset_1h()
                        >
                            "Última 1 hora"
                        </button>
                        <button
                            class="px-2.5 py-1 rounded bg-slate-100 hover:bg-slate-200 text-slate-700 font-medium transition"
                            on:click=move |_| apply_preset_today()
                        >
                            "Hoje"
                        </button>
                        <button
                            class="px-2.5 py-1 rounded bg-slate-100 hover:bg-slate-200 text-slate-700 font-medium transition"
                            on:click=move |_| apply_preset_7d()
                        >
                            "Últimos 7 dias"
                        </button>
                    </div>
                </div>
            </div>

            // Log table
            <div class="bg-white border border-slate-200 rounded-xl overflow-hidden shadow-sm">
                <table class="w-full text-left">
                    <thead class="bg-slate-50 border-b border-slate-200 text-xs font-semibold text-slate-500 uppercase">
                        <tr>
                            <th class="px-6 py-3">"ID"</th>
                            <th class="px-6 py-3">"Timestamp"</th>
                            <th class="px-6 py-3">"Queue / Stream"</th>
                            <th class="px-6 py-3">"Microservice"</th>
                            <th class="px-6 py-3">"Status"</th>
                            <th class="px-6 py-3">"Message ID"</th>
                            <th class="px-6 py-3">"Execution Time"</th>
                            <th class="px-6 py-3">"Actions"</th>
                        </tr>
                    </thead>
                    <tbody class="divide-y divide-slate-200 text-sm">
                        {move || {
                            let list = logs.get();
                            let total_len = list.len();
                            let page = logs_page.get();
                            let start = (page - 1) * 10;
                            let end = std::cmp::min(page * 10, total_len);

                            if total_len == 0 {
                                return view! {
                                    <tr>
                                        <td colspan="8" class="px-6 py-8 text-center text-slate-400">
                                            "No execution logs found"
                                        </td>
                                    </tr>
                                        }.into_any();
                            }

                            let page_slice = list[start..end].to_vec();
                            page_slice.into_iter().map(|log| {
                                let log_clone = log.clone();
                                let q_key = queues.get().into_iter().find(|q| q.id == Some(log.queue_id.clone()))
                                    .map(|q| q.stream_key).unwrap_or_else(|| "unknown_stream".to_string());
                                let ms_name = microservices.get().into_iter().find(|m| m.id == Some(log.microservice_id.clone()))
                                    .map(|m| m.name).unwrap_or_else(|| "unknown_service".to_string());
                                let log_id_val = log.id.clone().unwrap_or_default();
                                let reload_fn = reload.clone();
                                view! {
                                    <tr class="hover:bg-slate-50/50">
                                        <td class="px-6 py-4 font-mono text-xs text-slate-600">
                                            {log.id.clone().unwrap_or_default()}
                                        </td>
                                        <td class="px-6 py-4 font-mono text-xs text-slate-600">
                                            {format_relative_time(log.created_at)}
                                        </td>
                                        <td class="px-6 py-4 font-semibold text-slate-800 font-mono text-xs">
                                            {q_key}
                                        </td>
                                        <td class="px-6 py-4 text-slate-700 font-semibold">
                                            {ms_name}
                                        </td>
                                        <td class="px-6 py-4">
                                            <span class=format!("px-2 py-0.5 rounded text-xs font-semibold uppercase {}", if log.status == "success" { "bg-emerald-50 text-emerald-700 border border-emerald-200" } else { "bg-red-50 text-red-700 border border-red-200" })>
                                                {log.status.clone()}
                                            </span>
                                        </td>
                                        <td class="px-6 py-4 font-mono text-xs text-slate-500">{log.stream_message_id}</td>
                                        <td class="px-6 py-4 font-mono text-xs text-slate-500">{format!("{} ms", log.execution_time_ms)}</td>
                                        <td class="px-6 py-4 flex items-center gap-3">
                                            <button
                                                class="text-indigo-600 hover:text-indigo-800 font-semibold text-xs"
                                                on:click=move |_| selected_log.set(Some(log_clone.clone()))
                                            >
                                                "View"
                                            </button>
                                            <button
                                                class="text-red-600 hover:text-red-800 font-semibold text-xs"
                                                on:click=move |_| {
                                                    let log_id = log_id_val.clone();
                                                    let r_fn = reload_fn.clone();
                                                    spawn_local(async move {
                                                        if let Ok(_) = api::delete_log(&log_id).await {
                                                            r_fn();
                                                        }
                                                    });
                                                }
                                            >
                                                "Delete"
                                            </button>
                                        </td>
                                    </tr>
                                }
                            }).collect::<Vec<_>>().into_any()
                        }}
                    </tbody>
                </table>

                // Pagination controls for execution logs
                <div class="px-6 py-3 border-t border-slate-200 flex items-center justify-between bg-slate-50 text-xs font-semibold text-slate-500">
                    <div>
                        {move || {
                            let total_len = logs.get().len();
                            let total_pages = (total_len + 10 - 1) / 10;
                            format!("Page {} of {}", logs_page.get(), std::cmp::max(total_pages, 1))
                        }}
                    </div>
                    <div class="flex gap-2">
                        <button
                            class="px-2 py-1 bg-white border border-slate-200 rounded text-slate-600 disabled:opacity-50 hover:bg-slate-50 transition"
                            disabled=move || logs_page.get() <= 1
                            on:click=move |_| logs_page.set(logs_page.get() - 1)
                        >
                            "Previous"
                        </button>
                        <button
                            class="px-2 py-1 bg-white border border-slate-200 rounded text-slate-600 disabled:opacity-50 hover:bg-slate-50 transition"
                            disabled=move || {
                                let total_len = logs.get().len();
                                let total_pages = if total_len == 0 { 1 } else { (total_len + 10 - 1) / 10 };
                                logs_page.get() >= total_pages
                            }
                            on:click=move |_| logs_page.set(logs_page.get() + 1)
                        >
                            "Next"
                        </button>
                    </div>
                </div>
            </div>

            // Detailed JSON Modal
            {move || selected_log.get().map(|log| {
                let log_id_val = log.id.clone().unwrap_or_default();
                let payload_str = serde_json::to_string_pretty(&log.payload_input).unwrap_or_default();
                let is_error = log.status != "success";
                let reload_fn = reload.clone();
                view! {
                    <div class="fixed inset-0 z-50 flex items-center justify-center p-6 bg-slate-900/50">
                        <div class="bg-white border border-slate-200 rounded-xl max-w-2xl w-full p-6 space-y-4 shadow-2xl overflow-y-auto max-h-[85vh]">
                            <div class="flex justify-between items-center border-b border-slate-200 pb-3">
                                <h3 class="font-bold text-slate-900 text-lg">"Log Details"</h3>
                                <div class="flex gap-2">
                                    {if is_error {
                                        let log_id = log_id_val.clone();
                                        let r_fn = reload_fn.clone();
                                        view! {
                                            <button
                                                class="bg-slate-950 hover:bg-slate-900 text-white px-3 py-1.5 rounded-lg text-xs font-semibold shadow transition"
                                                on:click=move |_| {
                                                    let l_id = log_id.clone();
                                                    let r_fn2 = r_fn.clone();
                                                    spawn_local(async move {
                                                        if let Ok(_) = api::resend_log(&l_id).await {
                                                            r_fn2();
                                                            if let Some(w) = web_sys::window() {
                                                                let _ = w.alert_with_message("Payload resent to source queue successfully!");
                                                            }
                                                        }
                                                    });
                                                }
                                            >
                                                "Resend Payload"
                                            </button>
                                        }.into_any()
                                    } else {
                                        view! {}.into_any()
                                    }}
                                    <button
                                        class="bg-white hover:bg-slate-50 border border-slate-200 text-slate-700 px-3 py-1.5 rounded-lg text-xs font-semibold shadow-sm transition"
                                        on:click=move |_| selected_log.set(None)
                                    >
                                        "Close"
                                    </button>
                                </div>
                            </div>

                            <div class="grid grid-cols-2 gap-4 border-b border-slate-150 pb-4 text-sm text-slate-600">
                                <div>
                                    <span class="font-semibold text-slate-400 uppercase tracking-wider text-[10px] block mb-1">"Microservice"</span>
                                    <span class="font-bold text-slate-800">{
                                        let ms_id_c = &log.microservice_id;
                                        microservices.get().into_iter().find(|m| m.id.as_ref() == Some(ms_id_c))
                                            .map(|m| m.name).unwrap_or_else(|| format!("ID: {}", log.microservice_id))
                                    }</span>
                                </div>
                                <div>
                                    <span class="font-semibold text-slate-400 uppercase tracking-wider text-[10px] block mb-1">"Redis Stream"</span>
                                    <span class="font-mono text-slate-800 font-bold">{
                                        let q_id_c = &log.queue_id;
                                        queues.get().into_iter().find(|q| q.id.as_ref() == Some(q_id_c))
                                            .map(|q| q.stream_key).unwrap_or_else(|| format!("ID: {}", log.queue_id))
                                    }</span>
                                </div>
                            </div>

                            <div class="space-y-4">
                                <div>
                                    <div class="flex justify-between items-center mb-2">
                                        <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider">"Payload Input"</label>
                                        <button
                                            class="text-xs text-slate-950 hover:text-slate-900 font-semibold flex items-center gap-1"
                                            on:click={
                                                let payload_text = payload_str.clone();
                                                move |_| {
                                                    if let Some(w) = web_sys::window() {
                                                        let nav = w.navigator().clipboard();
                                                        let _ = nav.write_text(&payload_text);
                                                    }
                                                }
                                            }
                                        >
                                            "Copy Payload"
                                        </button>
                                    </div>
                                    <pre class="bg-slate-50 p-4 border border-slate-200 rounded-lg text-xs font-mono text-slate-800 overflow-x-auto">
                                        {payload_str.clone()}
                                    </pre>
                                </div>

                                <div>
                                    <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">"Payload Output"</label>
                                    <pre class="bg-slate-50 p-4 border border-slate-200 rounded-lg text-xs font-mono text-slate-800 overflow-x-auto">
                                        {log.payload_output.as_ref().map(|o| serde_json::to_string_pretty(o).unwrap_or_default()).unwrap_or_else(|| "None".to_string())}
                                    </pre>
                                </div>

                                {log.error_message.as_ref().map(|err| {
                                    let err_c = err.clone();
                                    view! {
                                        <div>
                                            <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">"Error Message"</label>
                                            <pre class="bg-red-50 p-4 border border-red-200 rounded-lg text-xs font-mono text-red-700 overflow-x-auto">
                                                {err_c}
                                            </pre>
                                        </div>
                                    }
                                })}
                            </div>
                        </div>
                    </div>
                }
            })}
        </div>
    }
}

#[component]
fn FlowView() -> impl IntoView {
    let queues = RwSignal::new(Vec::<QueueDTO>::new());
    let bindings = RwSignal::new(Vec::<BindingDTO>::new());
    let microservices = RwSignal::new(Vec::<MicroserviceDTO>::new());
    let filter_queue = RwSignal::new(String::new());
    let filter_microservice = RwSignal::new(String::new());

    let reload = move || {
        spawn_local(async move {
            if let Ok(q_list) = api::get_queues().await {
                queues.set(q_list);
            }
            if let Ok(b_list) = api::get_bindings().await {
                bindings.set(b_list);
            }
            if let Ok(m_list) = api::get_services().await {
                microservices.set(m_list);
            }
        });
    };

    Effect::new(move |_| {
        reload();
    });

    view! {
        <div class="p-8 space-y-6">
            <div class="flex justify-between items-center">
                <div>
                    <h1 class="text-2xl font-bold text-slate-900">"Workflow Canvas"</h1>
                    <p class="text-sm text-slate-500">"Visual representation of event-driven microservices pipelines"</p>
                </div>
                <button
                    class="bg-black hover:bg-zinc-900 text-white font-semibold py-2 px-4 rounded-lg text-sm transition shadow-sm"
                    on:click=move |_| reload()
                >
                    "Refresh Flow"
                </button>
            </div>

            // Filters Toolbar
            <div class="bg-white border border-slate-200 rounded-xl p-4 flex flex-wrap items-center gap-4 shadow-sm text-xs">
                <div class="flex items-center gap-2">
                    <span class="font-bold text-slate-600 uppercase tracking-wider text-[11px]">"Queue:"</span>
                    <select
                        class="bg-slate-50 border border-slate-300 rounded-lg px-3 py-1.5 text-slate-800 outline-none focus:border-indigo-500 font-mono text-xs"
                        prop:value=move || filter_queue.get()
                        on:change=move |ev| filter_queue.set(event_target_value(&ev))
                    >
                        <option value="" prop:selected=move || filter_queue.get().is_empty()>"All Queues"</option>
                        {move || queues.get().into_iter().map(|q| {
                            let sk = q.stream_key.clone();
                            let sk_val = sk.clone();
                            let sk_chk = sk.clone();
                            view! {
                                <option value=sk_val prop:selected=move || filter_queue.get() == sk_chk>{sk}</option>
                            }
                        }).collect::<Vec<_>>()}
                    </select>
                </div>

                <div class="flex items-center gap-2">
                    <span class="font-bold text-slate-600 uppercase tracking-wider text-[11px]">"Microservice:"</span>
                    <select
                        class="bg-slate-50 border border-slate-300 rounded-lg px-3 py-1.5 text-slate-800 outline-none focus:border-indigo-500 text-xs"
                        prop:value=move || filter_microservice.get()
                        on:change=move |ev| filter_microservice.set(event_target_value(&ev))
                    >
                        <option value="" prop:selected=move || filter_microservice.get().is_empty()>"All Microservices"</option>
                        {move || microservices.get().into_iter().map(|m| {
                            let m_id = m.id.unwrap_or_default();
                            let m_id_val = m_id.clone();
                            let m_id_chk = m_id.clone();
                            let m_name = m.name.clone();
                            view! {
                                <option value=m_id_val prop:selected=move || filter_microservice.get() == m_id_chk>{format!("{} (#{})", m_name, m_id)}</option>
                            }
                        }).collect::<Vec<_>>()}
                    </select>
                </div>

                {move || {
                    let has_filter = !filter_queue.get().is_empty() || !filter_microservice.get().is_empty();
                    if has_filter {
                        view! {
                            <button
                                class="text-indigo-600 hover:text-indigo-800 font-semibold px-2 py-1 bg-indigo-50 rounded border border-indigo-100 transition text-xs"
                                on:click=move |_| {
                                    filter_queue.set(String::new());
                                    filter_microservice.set(String::new());
                                }
                            >
                                "Clear Filters"
                            </button>
                        }.into_any()
                    } else {
                        view! { <span /> }.into_any()
                    }
                }}
            </div>

            <div class="space-y-8">
                {move || if bindings.get().is_empty() {
                    view! {
                        <div class="bg-white border border-slate-200 rounded-xl p-12 text-center text-slate-500 shadow-sm">
                            <svg class="w-12 h-12 mx-auto text-slate-300 mb-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13.828 10.172a4 4 0 00-5.656 0l-4 4a4 4 0 105.656 5.656l1.102-1.101m-.758-4.899a4 4 0 005.656 0l4-4a4 4 0 00-5.656-5.656l-1.1 1.1" />
                            </svg>
                            <p class="font-semibold text-slate-700 mb-1">"No Active Event Flows"</p>
                            <p class="text-xs text-slate-500">"Create queue-to-service bindings in the Redis Queues section to start visualizing."</p>
                        </div>
                    }.into_any()
                } else {
                    let b_list = bindings.get();
                    let ms_list = microservices.get();
                    let q_list = queues.get();
                    let f_q = filter_queue.get();
                    let f_ms = filter_microservice.get();

                    let mut start_bindings = Vec::new();
                    for b in &b_list {
                        let is_triggered_by_action = b_list.iter().any(|other_b| {
                            let s_dest = if other_b.on_success_action == "publish" {
                                other_b.on_success_config.as_str().map(|s| s.to_string())
                            } else if other_b.on_success_action == "key_event" {
                                other_b.on_success_config.get("target_stream").and_then(|v| v.as_str()).map(|s| s.to_string())
                            } else {
                                None
                            };

                            let e_dest = if other_b.on_error_action == "publish" {
                                other_b.on_error_config.as_str().map(|s| s.to_string())
                            } else if other_b.on_error_action == "key_event" {
                                other_b.on_error_config.get("target_stream").and_then(|v| v.as_str()).map(|s| s.to_string())
                            } else {
                                None
                            };

                            let current_q_stream = q_list.iter().find(|q| q.id == Some(b.queue_id.clone())).map(|q| q.stream_key.clone());
                            current_q_stream.is_some() && (s_dest == current_q_stream || e_dest == current_q_stream)
                        });

                        if !is_triggered_by_action {
                            start_bindings.push(b.clone());
                        }
                    }

                    let pipelines_starts = if start_bindings.is_empty() { b_list.clone() } else { start_bindings };

                    let mut all_pipelines = Vec::new();
                    let mut visited_binding_ids = std::collections::HashSet::new();

                    for start_binding in pipelines_starts {
                        let mut stages = Vec::new();
                        let mut current_binding = Some(start_binding);
                        let mut path_visited = std::collections::HashSet::new();

                        while let Some(b) = current_binding {
                            let b_id = b.id.clone().unwrap_or_default();
                            if !path_visited.insert(b_id.clone()) {
                                break;
                            }
                            visited_binding_ids.insert(b_id);

                            let q_key = q_list.iter().find(|q| q.id == Some(b.queue_id.clone()))
                                .map(|q| q.stream_key.clone()).unwrap_or_else(|| "unknown_stream".to_string());
                            
                            let ms = ms_list.iter().find(|m| m.id == Some(b.microservice_id.clone()) || m.name == b.microservice_id);
                            let ms_name = ms.as_ref().map(|m| m.name.clone()).unwrap_or_else(|| "unknown_service".to_string());
                            let ms_ver = ms.as_ref().and_then(|m| m.active_version_tag.clone()).unwrap_or_else(|| "no active version".to_string());
                            let ms_id = b.microservice_id.clone();

                            let s_action = b.on_success_action.clone();
                            let s_config = if s_action == "publish" {
                                b.on_success_config.as_str().unwrap_or("").to_string()
                            } else if s_action == "key_event" {
                                b.on_success_config.get("target_stream").and_then(|v| v.as_str()).unwrap_or("").to_string()
                            } else {
                                String::new()
                            };

                            let e_action = b.on_error_action.clone();
                            let e_config = if e_action == "publish" {
                                b.on_error_config.as_str().unwrap_or("").to_string()
                            } else if e_action == "key_event" {
                                b.on_error_config.get("target_stream").and_then(|v| v.as_str()).unwrap_or("").to_string()
                            } else {
                                String::new()
                            };

                            stages.push((q_key, ms_name, ms_ver, ms_id, s_action.clone(), s_config.clone(), e_action.clone(), e_config.clone()));

                            if (s_action == "publish" || s_action == "key_event") && !s_config.is_empty() {
                                let next_q = q_list.iter().find(|q| q.stream_key == s_config);
                                if let Some(nq) = next_q {
                                    current_binding = b_list.iter().find(|x| x.queue_id == nq.id.clone().unwrap_or_default()).cloned();
                                } else {
                                    current_binding = None;
                                }
                            } else {
                                current_binding = None;
                            }
                        }
                        if !stages.is_empty() {
                            all_pipelines.push(stages);
                        }
                    }

                    let filtered_pipelines: Vec<_> = all_pipelines.into_iter().filter(|stages| {
                        let matches_q = f_q.is_empty() || stages.iter().any(|(q, _, _, _, _, s_cfg, _, e_cfg)| {
                            q == &f_q || s_cfg == &f_q || e_cfg == &f_q
                        });
                        let matches_ms = f_ms.is_empty() || stages.iter().any(|(_, ms_name, _, ms_id, _, _, _, _)| {
                            ms_id == &f_ms || ms_name == &f_ms
                        });
                        matches_q && matches_ms
                    }).collect();

                    if filtered_pipelines.is_empty() {
                        return view! {
                            <div class="bg-white border border-slate-200 rounded-xl p-12 text-center text-slate-500 shadow-sm">
                                <p class="font-semibold text-slate-700 mb-1">"No Pipelines Match Current Filters"</p>
                                <p class="text-xs text-slate-500">"Try selecting a different queue or microservice filter above."</p>
                            </div>
                        }.into_any();
                    }

                    filtered_pipelines.into_iter().map(|stages| {
                        view! {
                            <div class="bg-white border border-slate-200 rounded-2xl p-6 shadow-sm hover:shadow-md transition space-y-4">
                                <h3 class="text-xs font-bold text-slate-400 uppercase tracking-wider border-b border-slate-100 pb-2">"Active Event Pipeline Lifecycle"</h3>
                                <div class="flex flex-wrap items-center gap-4 py-4 overflow-x-auto">
                                    {stages.into_iter().enumerate().map(|(idx, (q_key, ms_name, ms_ver, _ms_id, s_action, s_config, e_action, e_config))| {
                                        view! {
                                            {if idx > 0 {
                                                view! {
                                                    <div class="flex items-center text-slate-350 px-2">
                                                        <svg class="w-6 h-6 animate-pulse" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5">
                                                            <path stroke-linecap="round" stroke-linejoin="round" d="M13 5l7 7-7 7M5 5l7 7-7 7" />
                                                        </svg>
                                                    </div>
                                                }.into_any()
                                            } else {
                                                view! { <div /> }.into_any()
                                            }}

                                            <div class="flex flex-col md:flex-row items-center bg-slate-50/50 border border-slate-200 rounded-2xl p-4 gap-4 shadow-sm">
                                                // Stream Card
                                                <div class="bg-indigo-50 border border-indigo-200 rounded-xl p-3 flex flex-col space-y-1 w-44 shadow-inner">
                                                    <span class="text-[9px] font-extrabold uppercase tracking-wider text-indigo-500">"Redis Stream"</span>
                                                    <span class="text-slate-800 font-mono text-xs font-bold truncate">{q_key}</span>
                                                </div>

                                                // Connector
                                                <div class="text-slate-400">
                                                    <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                                        <path stroke-linecap="round" stroke-linejoin="round" d="M9 5l7 7-7 7" />
                                                    </svg>
                                                </div>

                                                // Microservice Card
                                                <div class="bg-white border border-slate-200 rounded-xl p-3 flex flex-col space-y-1 w-44 shadow-sm relative">
                                                    <div class="flex justify-between items-center">
                                                        <span class="text-[9px] font-extrabold uppercase tracking-wider text-slate-400">"Runner"</span>
                                                        <span class="px-1.5 py-0.5 bg-emerald-50 text-emerald-700 rounded text-[8px] font-bold border border-emerald-200 truncate">
                                                            {ms_ver}
                                                        </span>
                                                    </div>
                                                    <span class="text-slate-800 font-bold text-xs truncate">{ms_name}</span>
                                                </div>

                                                // Connector
                                                <div class="text-slate-400">
                                                    <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                                        <path stroke-linecap="round" stroke-linejoin="round" d="M9 5l7 7-7 7" />
                                                    </svg>
                                                </div>

                                                // Actions summary card
                                                <div class="bg-slate-100/80 border border-slate-200 rounded-xl p-3 flex flex-col space-y-2 w-44 shadow-sm text-xs">
                                                    <div>
                                                        <span class="text-[8px] font-bold text-emerald-600 block uppercase">"On Success"</span>
                                                        {if s_action == "publish" || s_action == "key_event" {
                                                            let s_lbl = if s_action == "key_event" { format!("Cond ➔ {}", s_config) } else { format!("➔ {}", s_config) };
                                                            view! {
                                                                <span class="font-mono text-[9px] font-bold text-slate-700 block truncate">{s_lbl}</span>
                                                            }.into_any()
                                                        } else {
                                                            view! {
                                                                <span class="text-[9px] text-slate-500 font-semibold block">"ACK Message"</span>
                                                            }.into_any()
                                                        }}
                                                    </div>
                                                    <div class="border-t border-slate-200 pt-1">
                                                        <span class="text-[8px] font-bold text-red-500 block uppercase">"On Error"</span>
                                                        {if e_action == "publish" || e_action == "key_event" {
                                                            let e_lbl = if e_action == "key_event" { format!("Cond ➔ {}", e_config) } else { format!("➔ {}", e_config) };
                                                            view! {
                                                                <span class="font-mono text-[9px] font-bold text-slate-700 block truncate">{e_lbl}</span>
                                                            }.into_any()
                                                        } else {
                                                            view! {
                                                                <span class="text-[9px] text-slate-500 font-semibold block">"Log Failure"</span>
                                                            }.into_any()
                                                        }}
                                                    </div>
                                                </div>
                                            </div>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>
                            </div>
                        }
                    }).collect::<Vec<_>>().into_any()
                }}
            </div>
        </div>
    }
}

#[component]
fn DocsView() -> impl IntoView {
    view! {
        <div class="p-8 space-y-6">
            <div>
                <h1 class="text-2xl font-bold text-slate-900">"Documentação do Desenvolvedor (Rust SDK)"</h1>
                <p class="text-sm text-slate-500">"Aprenda como receber dados, processar, interagir com bancos de dados e retornar outputs em microsserviços Rust."</p>
            </div>

            // 1. Entrada de Dados
            <div class="bg-white border border-slate-200 rounded-xl p-6 space-y-4 shadow-sm">
                <h2 class="text-lg font-bold text-slate-800">"1. Entrada de Dados (Input Payload)"</h2>
                <div class="space-y-2 text-sm text-slate-600 leading-relaxed">
                    <p>
                        "Em cada execução, o DooPack injeta o payload do evento na variável de ambiente "
                        <code class="bg-slate-100 text-indigo-600 px-1 py-0.5 rounded font-mono text-xs">"PAYLOAD_INPUT"</code>
                        " como uma string JSON."
                    </p>
                    <p>
                        "Utilizando o "
                        <code class="bg-slate-100 text-indigo-600 px-1 py-0.5 rounded font-mono text-xs">"rust-sdk"</code>
                        ", você pode ler e deserializar o payload diretamente em uma struct do Rust usando a biblioteca "
                        <code class="bg-slate-100 text-indigo-600 px-1 py-0.5 rounded font-mono text-xs">"serde"</code>
                        ":"
                    </p>
                    <pre class="bg-slate-50 border border-slate-200 rounded-lg p-4 font-mono text-xs text-slate-800 overflow-x-auto">
{"use serde::Deserialize;\n\n#[derive(Deserialize)]\n#[serde(rename_all = \"camelCase\")]\nstruct InputData {\n    pub user_id: i64,\n    pub amount: f64,\n    pub message: Option<String>,\n}\n\nfn main() {\n    // Lê o payload JSON vindo do orquestrador\n    let payload_val = rust_sdk::get_input().expect(\"Nenhum payload de entrada configurado\");\n    let input: InputData = serde_json::from_value(payload_val).expect(\"Erro ao parsear entrada\");\n    \n    println!(\"Processando transação para usuário ID: {}\", input.user_id);\n}"}
                    </pre>
                </div>
            </div>

            // 2. Saída de Dados
            <div class="bg-white border border-slate-200 rounded-xl p-6 space-y-4 shadow-sm">
                <h2 class="text-lg font-bold text-slate-800">"2. Saída de Dados (Output Response)"</h2>
                <div class="space-y-2 text-sm text-slate-600 leading-relaxed">
                    <p>
                        "Para que o orquestrador classifique a execução como concluída com sucesso, o microsserviço precisa retornar uma string JSON válida no "
                        <code class="bg-slate-100 text-indigo-600 px-1 py-0.5 rounded font-mono text-xs">"stdout"</code>
                        " e sair com código de retorno "
                        <code class="bg-slate-100 text-indigo-600 px-1 py-0.5 rounded font-mono text-xs">"0"</code>
                        "."
                    </p>
                    <p>
                        "O "
                        <code class="bg-slate-100 text-indigo-600 px-1 py-0.5 rounded font-mono text-xs">"rust-sdk"</code>
                        " simplifica isso ao providenciar a função "
                        <code class="bg-slate-100 text-indigo-600 px-1 py-0.5 rounded font-mono text-xs">"send_output"</code>
                        ", que serializa qualquer struct que implemente "
                        <code class="bg-slate-100 text-indigo-600 px-1 py-0.5 rounded font-mono text-xs">"Serialize"</code>
                        " e imprime para a saída padrão:"
                    </p>
                    <pre class="bg-slate-50 border border-slate-200 rounded-lg p-4 font-mono text-xs text-slate-800 overflow-x-auto">
{"use serde::Serialize;\n\n#[derive(Serialize)]\nstruct OutputData {\n    pub status: String,\n    pub total: f64,\n    pub date: String,\n}\n\nfn main() {\n    let response = OutputData {\n        status: \"success\".to_string(),\n        total: 150.50,\n        date: \"2026-08-27\".to_string(),\n    };\n    \n    // Serializa e envia a resposta de volta ao DooPack\n    rust_sdk::send_output(&response);\n}"}
                    </pre>
                </div>
            </div>

            // 3. Uso do Banco de Dados
            <div class="bg-white border border-slate-200 rounded-xl p-6 space-y-4 shadow-sm">
                <h2 class="text-lg font-bold text-slate-800">"3. Conexão e Uso de Banco de Dados"</h2>
                <div class="space-y-4 text-sm text-slate-600 leading-relaxed">
                    <p>
                        "Como os microsserviços rodam em containers efêmeros (Serverless), eles são iniciados para processar uma única mensagem e destruídos imediatamente após a saída do processo."
                    </p>
                    <div class="bg-slate-50 border-l-4 border-amber-500 p-4 text-xs text-slate-700 rounded-r-lg">
                        <span class="font-bold text-amber-700 block mb-1">"Ciclo de Vida de Conexões"</span>
                        "Você não pode manter conexões persistentes em memória compartilhada entre mensagens. Cada execução deve iniciar um pool temporário, rodar as queries necessárias, e encerrar o pool de forma limpa antes de finalizar."
                    </div>
                    <div>
                        <h3 class="font-bold text-slate-800 mb-2">"Exemplo Completo: CRUD no SurrealDB + Modelos Serde"</h3>
                        <pre class="bg-slate-50 border border-slate-200 rounded-lg p-4 font-mono text-xs text-slate-800 overflow-x-auto">
{"use serde::{Deserialize, Serialize};\n\n#[derive(Debug, Serialize, Deserialize)]\nstruct Order {\n    id: Option<String>,\n    user_id: String,\n    item_name: String,\n    quantity: i32,\n    status: String,\n}\n\n#[derive(Debug, Deserialize)]\nstruct ActionInput {\n    pub action: String, // \"create\", \"read\", \"update\", \"delete\"\n    pub order_id: Option<String>,\n    pub user_id: Option<String>,\n    pub item_name: Option<String>,\n    pub quantity: Option<i32>,\n}\n\n#[derive(Debug, Serialize)]\nstruct ActionOutput {\n    pub success: bool,\n    pub message: String,\n    pub data: Option<Order>,\n}\n\n#[tokio::main]\nasync fn main() -> Result<(), Box<dyn std::error::Error>> {\n    // 1. Ingestão dos parâmetros de entrada\n    let payload = rust_sdk::get_input().expect(\"Nenhum input recebido\");\n    let input: ActionInput = serde_json::from_value(payload)?;\n\n    // 2. Conecta ao SurrealDB em uma única chamada usando o SDK (resolve credenciais, namespace e database automaticamente)\n    let db = rust_sdk::connect_surreal(\"surrealdb-pool\").await?;\n\n    let mut success = false;\n    let mut message = \"Ação desconhecida\".to_string();\n    let mut result_data: Option<Order> = None;\n\n    // 3. Execução do CRUD com base na ação enviada\n    match input.action.as_str() {\n        \"create\" => {\n            let new_order = Order {\n                id: None,\n                user_id: input.user_id.unwrap_or_default(),\n                item_name: input.item_name.unwrap_or_default(),\n                quantity: input.quantity.unwrap_or(1),\n                status: \"pending\".to_string(),\n            };\n            // CREATE: Insere o modelo diretamente no banco\n            let created: Option<Order> = db.create(\"orders\").content(&new_order).await?;\n            result_data = created;\n            success = true;\n            message = \"Registro criado com sucesso!\".to_string();\n        }\n        \"read\" => {\n            if let Some(ref id) = input.order_id {\n                // READ: Busca o documento pelo ID e deserializa de volta à struct\n                let record: Option<Order> = db.select((\"orders\", id)).await?;\n                result_data = record;\n                success = result_data.is_some();\n                message = if success { \"Sucesso\".to_string() } else { \"Pedido não encontrado\".to_string() };\n            }\n        }\n        \"update\" => {\n            if let Some(ref id) = input.order_id {\n                // UPDATE: Mescla mudanças no modelo existente\n                let updated: Option<Order> = db.update((\"orders\", id))\n                    .merge(serde_json::json!({ \"status\": \"processed\" }))\n                    .await?;\n                result_data = updated;\n                success = true;\n                message = \"Status do pedido atualizado para processed!\".to_string();\n            }\n        }\n        \"delete\" => {\n            if let Some(ref id) = input.order_id {\n                // DELETE: Deleta o documento do banco\n                let deleted: Option<Order> = db.delete((\"orders\", id)).await?;\n                result_data = deleted;\n                success = true;\n                message = \"Pedido removido com sucesso!\".to_string();\n            }\n        }\n        _ => {}\n    }\n\n    // 4. Retorno dos resultados para o fluxo orquestrador\n    let output = ActionOutput {\n        success,\n        message,\n        data: result_data,\n    };\n    rust_sdk::send_output(&output);\n\n    Ok(())\n}"}
                        </pre>
                    </div>
                </div>
            </div>

            // 4. Variáveis de Ambiente
            <div class="bg-white border border-slate-200 rounded-xl p-6 space-y-4 shadow-sm">
                <h2 class="text-lg font-bold text-slate-800">"4. Variáveis de Ambiente (Envs)"</h2>
                <div class="space-y-4 text-sm text-slate-600 leading-relaxed">
                    <p>
                        "Variáveis de ambiente customizadas podem ser associadas a cada versão do microsserviço no painel do DooPack (na janela modal de "
                        <span class="font-semibold text-indigo-600">"Deploy Version"</span>
                        " sob a aba "
                        <span class="font-semibold text-slate-800">"Manage Environment Variables"</span>
                        "). Elas são salvas em formato JSON, por exemplo:"
                    </p>
                    <pre class="bg-slate-50 border border-slate-200 rounded-lg p-3 font-mono text-xs text-indigo-600">
                        {"{\n  \"API_TOKEN\": \"my-super-secret-token\",\n  \"EXTERNAL_SERVICE_URL\": \"https://api.externa.com/v1\"\n}"}
                    </pre>
                    <p>
                        "Para acessar esses valores dentro do seu código Rust, utilize o módulo padrão "
                        <code class="bg-slate-100 text-indigo-600 px-1 py-0.5 rounded font-mono text-xs">"std::env"</code>
                        ":"
                    </p>
                    <pre class="bg-slate-50 border border-slate-200 rounded-lg p-4 font-mono text-xs text-slate-800 overflow-x-auto">
{"use std::env;\n\nfn main() {\n    // 1. Obtendo variáveis de ambiente do sistema\n    let api_token = env::var(\"API_TOKEN\")\n        .expect(\"A variável de ambiente API_TOKEN não foi configurada\");\n\n    let external_url = env::var(\"EXTERNAL_SERVICE_URL\")\n        .unwrap_or_else(|_| \"https://sandbox.externa.com/v1\".to_string());\n\n    println!(\"Chamando serviço em {} usando o token {}\", external_url, api_token);\n}"}
                    </pre>
                    <p>
                        "Ao disparar execuções de testes ou via Redis Stream, você pode selecionar qual ambiente usar (como "
                        <code class="bg-slate-100 text-indigo-600 px-1 py-0.5 rounded font-mono text-xs">"prod"</code>
                        " ou "
                        <code class="bg-slate-100 text-indigo-600 px-1 py-0.5 rounded font-mono text-xs">"dev"</code>
                        ") passando a chave de configuração especial no payload:"
                    </p>
                    <pre class="bg-slate-50 border border-slate-200 rounded-lg p-3 font-mono text-xs text-slate-800 overflow-x-auto">
{"{\n  \"doopack\": {\n    \"env\": \"prod\"\n  },\n  \"a\": 10,\n  \"b\": 20\n}"}
                    </pre>
                </div>
            </div>

            // 5. Referência Completa da API HTTP
            <div class="bg-white border border-slate-200 rounded-xl p-6 space-y-6 shadow-sm text-slate-700">
                <div>
                    <h2 class="text-lg font-bold text-slate-800">"5. Referência da API HTTP (Endpoints e Formatos)"</h2>
                    <p class="text-sm text-slate-500 mt-1">"Todas as requisições devem incluir o cabeçalho de autenticação: "
                        <code class="bg-slate-100 text-indigo-600 px-1 py-0.5 rounded font-mono text-xs">"X-API-Key: dp_..."</code>
                        " ou "
                        <code class="bg-slate-100 text-indigo-600 px-1 py-0.5 rounded font-mono text-xs">"Authorization: Bearer dp_..."</code>
                        "."
                    </p>
                </div>

                <div class="divide-y divide-slate-100 space-y-6 text-sm">
                    // Route 1: Listar Variáveis de Ambiente
                    <div class="pt-4 space-y-2">
                        <div class="flex items-center gap-2">
                            <span class="bg-green-100 text-green-800 font-bold px-2 py-0.5 rounded text-xs font-mono">"GET"</span>
                            <span class="font-mono text-slate-900 font-semibold">"/api/v1/services/:id/envs"</span>
                        </div>
                        <p class="text-xs text-slate-500">"Retorna a lista de todas as variáveis de ambiente cadastradas para o microsserviço."</p>
                        <div class="bg-slate-50 border border-slate-200 rounded p-3 font-mono text-xs text-slate-800 overflow-x-auto">
                            <span class="font-semibold block text-slate-500 mb-1">"Response (Sucesso):"</span>
                            {"[\n  {\n    \"id\": \"5\",\n    \"microservice_id\": \"1\",\n    \"name\": \"prod\",\n    \"config\": {\n      \"DATABASE_URL\": \"mongodb://production_host:27017\",\n      \"API_KEY\": \"prod_secret_key\"\n    },\n    \"is_default\": true\n  }\n]"}
                        </div>
                    </div>

                    // Route 2: Cadastrar Configurações de Ambiente
                    <div class="pt-4 space-y-2">
                        <div class="flex items-center gap-2">
                            <span class="bg-indigo-100 text-indigo-800 font-bold px-2 py-0.5 rounded text-xs font-mono">"POST"</span>
                            <span class="font-mono text-slate-900 font-semibold">"/api/v1/services/:id/envs"</span>
                        </div>
                        <p class="text-xs text-slate-500">"Cria uma nova configuração de ambiente para o microsserviço (o microservice_id é opcional no payload)."</p>
                        <div class="bg-slate-50 border border-slate-200 rounded p-3 font-mono text-xs text-slate-800 overflow-x-auto">
                            <span class="font-semibold block text-slate-500 mb-1">"Request Body:"</span>
                            {"{\n  \"name\": \"prod\",\n  \"config\": {\n    \"DATABASE_URL\": \"mongodb://production_host:27017\"\n  },\n  \"is_default\": true\n}"}
                        </div>
                        <div class="bg-slate-50 border border-slate-200 rounded p-3 font-mono text-xs text-slate-800 overflow-x-auto">
                            <span class="font-semibold block text-slate-500 mb-1">"Response (Sucesso - retorna o objeto salvo):"</span>
                            {"{\n  \"id\": \"5\",\n  \"microservice_id\": \"1\",\n  \"name\": \"prod\",\n  \"config\": {\n    \"DATABASE_URL\": \"mongodb://production_host:27017\"\n  },\n  \"is_default\": true\n}"}
                        </div>
                    </div>

                    // Route 3: Editar Configuração de Ambiente
                    <div class="pt-4 space-y-2">
                        <div class="flex items-center gap-2">
                            <span class="bg-indigo-100 text-indigo-800 font-bold px-2 py-0.5 rounded text-xs font-mono">"POST"</span>
                            <span class="font-mono text-slate-900 font-semibold">"/api/v1/services/:id/envs/:env_id/edit"</span>
                        </div>
                        <p class="text-xs text-slate-500">"Edita uma configuração de ambiente existente pelo seu ID."</p>
                        <div class="bg-slate-50 border border-slate-200 rounded p-3 font-mono text-xs text-slate-800 overflow-x-auto">
                            <span class="font-semibold block text-slate-500 mb-1">"Request Body:"</span>
                            {"{\n  \"name\": \"prod-updated\",\n  \"config\": {\n    \"DATABASE_URL\": \"mongodb://prod_updated:27017\"\n  },\n  \"is_default\": true\n}"}
                        </div>
                        <div class="bg-slate-50 border border-slate-200 rounded p-3 font-mono text-xs text-slate-800 overflow-x-auto">
                            <span class="font-semibold block text-slate-500 mb-1">"Response (Sucesso - retorna o objeto atualizado):"</span>
                            {"{\n  \"id\": \"5\",\n  \"microservice_id\": \"1\",\n  \"name\": \"prod-updated\",\n  \"config\": {\n    \"DATABASE_URL\": \"mongodb://prod_updated:27017\"\n  },\n  \"is_default\": true\n}"}
                        </div>
                    </div>

                    // Route 4: Deletar Configuração de Ambiente
                    <div class="pt-4 space-y-2">
                        <div class="flex items-center gap-2">
                            <span class="bg-red-100 text-red-800 font-bold px-2 py-0.5 rounded text-xs font-mono">"DELETE"</span>
                            <span class="font-mono text-slate-900 font-semibold">"/api/v1/services/:id/envs/:env_id"</span>
                        </div>
                        <p class="text-xs text-slate-500">"Deleta uma variável de ambiente pelo seu ID único de banco."</p>
                        <div class="bg-slate-50 border border-slate-200 rounded p-3 font-mono text-xs text-slate-800 overflow-x-auto">
                            <span class="font-semibold block text-slate-500 mb-1">"Response (Sucesso):"</span>
                            {"Status: 204 No Content"}
                        </div>
                    </div>
                </div>
            </div>
        </div>
    }
}

#[component]
fn ApiKeysView() -> impl IntoView {
    let keys = RwSignal::new(Vec::<ApiKeyDTO>::new());
    let new_key_name = RwSignal::new(String::new());

    let reload = move || {
        spawn_local(async move {
            if let Ok(list) = api::get_api_keys().await {
                keys.set(list);
            }
        });
    };

    reload();

    view! {
        <div class="p-8 space-y-6">
            <div>
                <h1 class="text-2xl font-bold text-slate-900">"API Keys Manager"</h1>
                <p class="text-sm text-slate-500">"Generate and manage API keys to authenticate external HTTP requests to DooPack."</p>
            </div>

            <div class="grid grid-cols-1 md:grid-cols-3 gap-6">
                // Left: Keys list
                <div class="md:col-span-2 space-y-4">
                    <div class="bg-white border border-slate-200 rounded-xl shadow-sm overflow-hidden text-sm">
                        <table class="w-full text-left">
                            <thead class="bg-slate-100 text-xs font-semibold text-slate-500 uppercase border-b border-slate-200">
                                <tr>
                                    <th class="px-6 py-3">"Name"</th>
                                    <th class="px-6 py-3">"Token/Key"</th>
                                    <th class="px-6 py-3">"Created At"</th>
                                    <th class="px-6 py-3 text-right">"Action"</th>
                                </tr>
                            </thead>
                            <tbody class="divide-y divide-slate-200">
                                {move || {
                                    let list = keys.get();
                                    if list.is_empty() {
                                        view! {
                                            <tr>
                                                <td colspan="4" class="px-6 py-8 text-center text-slate-400">
                                                    "No API Keys generated yet"
                                                </td>
                                            </tr>
                                        }.into_any()
                                    } else {
                                        list.into_iter().map(|k| {
                                            let k_id = k.id.clone().unwrap_or_default();
                                            let name = k.name.clone();
                                            let key_val = k.key_value.clone().unwrap_or_default();
                                            let created_str = k.created_at
                                                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                                                .unwrap_or_else(|| "N/A".to_string());
                                            
                                            view! {
                                                <tr class="hover:bg-slate-50/50">
                                                    <td class="px-6 py-4 font-bold text-slate-800">{name}</td>
                                                    <td class="px-6 py-4">
                                                         <div class="flex items-center gap-2">
                                                             <span class="font-mono text-xs text-slate-600 bg-slate-50/50 select-all border border-slate-100 rounded px-2 py-1 max-w-[200px] truncate">
                                                                 {key_val.clone()}
                                                             </span>
                                                             <button
                                                                 class="p-1 hover:bg-slate-200 rounded text-slate-500 hover:text-slate-700 transition"
                                                                 title="Copy Key"
                                                                 on:click={
                                                                     let key_val_c = key_val.clone();
                                                                     move |_| {
                                                                         if let Some(window) = web_sys::window() {
                                                                             let navigator = window.navigator();
                                                                             let clipboard = navigator.clipboard();
                                                                             let _ = clipboard.write_text(&key_val_c);
                                                                         }
                                                                     }
                                                                 }
                                                             >
                                                                 <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                                     <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 5H6a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2v-1M8 5a2 2 0 002 2h2a2 2 0 002-2M8 5a2 2 0 012-2h2a2 2 0 012 2m0 0h2a2 2 0 012 2v3m2 4H10m0 0l3-3m-3 3l3 3" />
                                                                 </svg>
                                                             </button>
                                                         </div>
                                                     </td>
                                                    <td class="px-6 py-4 text-xs text-slate-500">{created_str}</td>
                                                    <td class="px-6 py-4 text-right">
                                                        <button
                                                            class="bg-red-50 hover:bg-red-100 border border-red-200 text-red-600 font-semibold px-2 py-1 rounded text-xs transition"
                                                            on:click={
                                                                let k_id = k_id.clone();
                                                                move |_| {
                                                                    let k_id = k_id.clone();
                                                                    spawn_local(async move {
                                                                        let _ = api::delete_api_key(&k_id).await;
                                                                        reload();
                                                                    });
                                                                }
                                                            }
                                                        >
                                                            "Delete"
                                                        </button>
                                                    </td>
                                                </tr>
                                            }
                                        }).collect::<Vec<_>>().into_any()
                                    }
                                }}
                            </tbody>
                        </table>
                    </div>
                </div>

                // Right: Add API Key Form
                <div class="bg-white border border-slate-200 rounded-xl p-6 space-y-4 shadow-sm h-fit">
                    <h3 class="font-bold text-slate-800 text-md">"Generate API Key"</h3>
                    <div>
                        <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">"Key Identifier/Name"</label>
                        <input
                            type="text"
                            placeholder="e.g. CI/CD Pipeline, External System"
                            class="w-full bg-slate-50 border border-slate-300 rounded-lg px-3 py-2 text-slate-900 outline-none focus:border-indigo-500 text-sm"
                            on:input=move |ev| new_key_name.set(event_target_value(&ev))
                            prop:value=new_key_name
                        />
                    </div>
                    <button
                        class="w-full bg-black hover:bg-zinc-900 text-white font-semibold py-2 rounded-lg text-sm transition shadow-sm"
                        on:click=move |_| {
                            let name = new_key_name.get();
                            if name.is_empty() { return; }
                            spawn_local(async move {
                                if let Ok(_) = api::create_api_key(&name).await {
                                    new_key_name.set(String::new());
                                    reload();
                                }
                            });
                        }
                    >
                        "Generate Key"
                    </button>
                </div>
            </div>
        </div>
    }
}

#[component]
fn SchedulesView() -> impl IntoView {
    let schedules = RwSignal::new(Vec::<ScheduledJobDTO>::new());
    let services = RwSignal::new(Vec::<MicroserviceDTO>::new());
    
    let selected_service_id = RwSignal::new(String::new());
    let schedule_type = RwSignal::new("delay".to_string()); // "delay", "datetime", "cron"
    let delay_input = RwSignal::new("60".to_string());
    let datetime_input = RwSignal::new(String::new());
    let cron_input = RwSignal::new("0 7 * * *".to_string());
    let payload_input = RwSignal::new("{\n  \"a\": 10,\n  \"b\": 20\n}".to_string());

    let reload = move || {
        spawn_local(async move {
            if let Ok(list) = api::get_schedules().await {
                schedules.set(list);
            }
            if let Ok(s_list) = api::get_services().await {
                services.set(s_list);
            }
        });
    };

    Effect::new(move |_| {
        reload();
    });

    view! {
        <div class="p-8 space-y-6">
            <div>
                <h1 class="text-2xl font-bold text-slate-900">"Scheduled Executions"</h1>
                <p class="text-sm text-slate-500">"Schedule microservices to run automatically at a specific time or with a relative delay"</p>
            </div>

            <div class="grid grid-cols-1 lg:grid-cols-3 gap-6">
                // Left Panel: Form
                <div class="bg-white border border-slate-200 rounded-xl p-6 space-y-4 shadow-sm h-fit">
                    <h3 class="font-bold text-slate-800 text-md">"Schedule New Job"</h3>
                    
                    <div class="space-y-3">
                        <div>
                            <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">"Microservice"</label>
                            <select
                                class="w-full bg-slate-50 border border-slate-300 rounded-lg px-3 py-2 text-slate-900 outline-none focus:border-indigo-500 text-sm"
                                on:change=move |ev| selected_service_id.set(event_target_value(&ev))
                                prop:value=selected_service_id
                            >
                                <option value="">"Select Microservice..."</option>
                                {move || services.get().into_iter().map(|s| {
                                    let s_id = s.id.clone().unwrap_or_default();
                                    view! {
                                        <option value=s_id.clone()>{s.name.clone()}</option>
                                    }
                                }).collect::<Vec<_>>()}
                            </select>
                        </div>

                        <div>
                            <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">"Schedule Method"</label>
                            <div class="flex flex-col md:flex-row gap-4">
                                <label class="inline-flex items-center gap-1.5 text-sm text-slate-700 cursor-pointer">
                                    <input
                                        type="radio"
                                        name="sched_method"
                                        value="delay"
                                        prop:checked=move || schedule_type.get() == "delay"
                                        on:change=move |_| schedule_type.set("delay".to_string())
                                    />
                                    "Delay"
                                </label>
                                <label class="inline-flex items-center gap-1.5 text-sm text-slate-700 cursor-pointer">
                                    <input
                                        type="radio"
                                        name="sched_method"
                                        value="datetime"
                                        prop:checked=move || schedule_type.get() == "datetime"
                                        on:change=move |_| schedule_type.set("datetime".to_string())
                                    />
                                    "Specific Time"
                                </label>
                                <label class="inline-flex items-center gap-1.5 text-sm text-slate-700 cursor-pointer">
                                    <input
                                        type="radio"
                                        name="sched_method"
                                        value="cron"
                                        prop:checked=move || schedule_type.get() == "cron"
                                        on:change=move |_| schedule_type.set("cron".to_string())
                                    />
                                    "Cron Expression"
                                </label>
                            </div>
                        </div>

                        {move || {
                            let s_type = schedule_type.get();
                            if s_type == "delay" {
                                view! {
                                    <div>
                                        <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">"Delay (Seconds)"</label>
                                        <input
                                            type="number"
                                            placeholder="e.g. 60"
                                            class="w-full bg-slate-50 border border-slate-300 rounded-lg px-3 py-2 text-slate-900 outline-none focus:border-indigo-500 text-sm"
                                            on:input=move |ev| delay_input.set(event_target_value(&ev))
                                            prop:value=delay_input
                                        />
                                    </div>
                                }.into_any()
                            } else if s_type == "datetime" {
                                view! {
                                    <div>
                                        <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">"Date & Time (UTC)"</label>
                                        <input
                                            type="datetime-local"
                                            class="w-full bg-slate-50 border border-slate-300 rounded-lg px-3 py-2 text-slate-900 outline-none focus:border-indigo-500 text-sm"
                                            on:input=move |ev| datetime_input.set(event_target_value(&ev))
                                            prop:value=datetime_input
                                        />
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <div>
                                        <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">"Cron Expression"</label>
                                        <input
                                            type="text"
                                            placeholder="e.g. 0 7 * * *"
                                            class="w-full bg-slate-50 border border-slate-300 rounded-lg px-3 py-2 text-slate-900 outline-none focus:border-indigo-500 text-sm"
                                            on:input=move |ev| cron_input.set(event_target_value(&ev))
                                            prop:value=cron_input
                                        />
                                        <p class="text-[10px] text-slate-400 mt-1">"Format: min hour day month day-of-week (e.g. 0 7 * * * to run daily at 07:00)"</p>
                                    </div>
                                }.into_any()
                            }
                        }}

                        <div>
                            <label class="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">"Payload Input (JSON)"</label>
                            <textarea
                                rows="4"
                                placeholder="{\n  \"a\": 10\n}"
                                class="w-full bg-slate-50 border border-slate-300 rounded-lg px-3 py-2 text-slate-900 outline-none focus:border-indigo-500 font-mono text-xs"
                                on:input=move |ev| payload_input.set(event_target_value(&ev))
                                prop:value=payload_input
                            />
                        </div>

                        <button
                            class="w-full bg-black hover:bg-zinc-900 text-white font-semibold py-2 rounded-lg text-sm transition shadow-sm"
                            on:click=move |_| {
                                let s_id = selected_service_id.get();
                                if s_id.is_empty() { return; }

                                let pl_str = payload_input.get();
                                let parsed_payload = match serde_json::from_str::<serde_json::Value>(&pl_str) {
                                    Ok(v) => v,
                                    Err(_) => {
                                        if let Some(w) = web_sys::window() {
                                            let _ = w.alert_with_message("Invalid JSON payload");
                                        }
                                        return;
                                    }
                                };

                                let method = schedule_type.get();
                                let mut req = ScheduleJobRequest {
                                    run_at: None,
                                    delay_seconds: None,
                                    cron_expression: None,
                                    payload: parsed_payload,
                                };

                                if method == "delay" {
                                    let secs = delay_input.get().parse::<i64>().unwrap_or(60);
                                    req.delay_seconds = Some(secs);
                                } else if method == "cron" {
                                    let expr = cron_input.get();
                                    if expr.trim().is_empty() { return; }
                                    req.cron_expression = Some(expr);
                                } else {
                                    let dt_str = datetime_input.get();
                                    if dt_str.is_empty() { return; }
                                    // Parse local datetime-local value (which lacks offset) into UTC DateTime
                                    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(&format!("{}:00", dt_str.replace('T', " ")), "%Y-%m-%d %H:%M:%S") {
                                        let utc_dt = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(naive, chrono::Utc);
                                        req.run_at = Some(utc_dt);
                                    } else {
                                        if let Some(w) = web_sys::window() {
                                            let _ = w.alert_with_message("Invalid DateTime format");
                                        }
                                        return;
                                    }
                                }

                                spawn_local(async move {
                                    if let Ok(_) = api::schedule_job(&s_id, req).await {
                                        reload();
                                    }
                                });
                            }
                        >
                            "Schedule Job"
                        </button>
                    </div>
                </div>

                // Right Panel: Table of Scheduled Jobs
                <div class="lg:col-span-2 space-y-4">
                    <div class="bg-white border border-slate-200 rounded-xl overflow-hidden shadow-sm">
                        <table class="w-full text-left border-collapse text-xs">
                            <thead class="bg-slate-50 border-b border-slate-200 text-[10px] font-semibold text-slate-500 uppercase">
                                <tr>
                                    <th class="px-6 py-3">"ID"</th>
                                    <th class="px-6 py-3">"Microservice"</th>
                                    <th class="px-6 py-3">"Scheduled Run Time"</th>
                                    <th class="px-6 py-3">"Payload"</th>
                                    <th class="px-6 py-3">"Status"</th>
                                    <th class="px-6 py-3 text-right">"Action"</th>
                                </tr>
                            </thead>
                            <tbody class="divide-y divide-slate-200 text-slate-600">
                                {move || {
                                    let s_list = schedules.get();
                                    let m_list = services.get();
                                    if s_list.is_empty() {
                                        view! {
                                            <tr>
                                                <td colspan="6" class="px-6 py-8 text-center text-slate-400">
                                                    "No executions scheduled yet"
                                                </td>
                                            </tr>
                                        }.into_any()
                                    } else {
                                        s_list.into_iter().map(|job| {
                                            let job_id = job.id.clone().unwrap_or_default();
                                            let job_id_c = job_id.clone();
                                            let service_name = m_list.iter()
                                                .find(|m| m.id.as_ref().map(|id| id == &job.microservice_id).unwrap_or(false))
                                                .map(|m| m.name.clone())
                                                .unwrap_or_else(|| format!("ID: {}", job.microservice_id));
                                            
                                            let run_time = job.run_at.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string();
                                            
                                            let cron_badge = if let Some(ref expr) = job.cron_expression {
                                                view! {
                                                    <span class="inline-block mt-1 px-1.5 py-0.5 bg-purple-50 text-purple-700 border border-purple-200 rounded text-[9px] font-mono font-bold">
                                                        {format!("CRON: {}", expr)}
                                                    </span>
                                                }.into_any()
                                            } else {
                                                view! { <div /> }.into_any()
                                            };

                                            let status_class = match job.status.as_str() {
                                                "pending" => "bg-blue-50 text-blue-700 border-blue-200",
                                                "completed" => "bg-emerald-50 text-emerald-700 border-emerald-200",
                                                "failed" => "bg-red-50 text-red-700 border-red-200",
                                                _ => "bg-slate-50 text-slate-700 border-slate-200",
                                            };

                                            view! {
                                                <tr class="hover:bg-slate-50/50">
                                                    <td class="px-6 py-4 font-mono font-bold text-slate-700">{job_id}</td>
                                                    <td class="px-6 py-4 font-bold text-slate-900">{service_name}</td>
                                                    <td class="px-6 py-4">
                                                        <div class="flex flex-col items-start">
                                                            <span>{run_time}</span>
                                                            {cron_badge}
                                                        </div>
                                                    </td>
                                                    <td class="px-6 py-4 font-mono max-w-xs truncate">{serde_json::to_string(&job.payload).unwrap_or_default()}</td>
                                                    <td class="px-6 py-4">
                                                        <span class=format!("px-1.5 py-0.5 border rounded text-[10px] font-bold {}", status_class)>
                                                            {job.status.to_uppercase()}
                                                        </span>
                                                    </td>
                                                    <td class="px-6 py-4 text-right">
                                                        {if job.status == "pending" {
                                                            let job_id_c = job_id_c.clone();
                                                            view! {
                                                                <button
                                                                    class="text-red-600 hover:text-red-800 font-bold transition text-xs"
                                                                    on:click=move |_| {
                                                                        let j_id = job_id_c.clone();
                                                                        spawn_local(async move {
                                                                            let _ = api::delete_schedule(&j_id).await;
                                                                            reload();
                                                                        });
                                                                    }
                                                                >
                                                                    "Cancel"
                                                                </button>
                                                            }.into_any()
                                                        } else {
                                                            view! { <span class="text-slate-400 select-none">"-"</span> }.into_any()
                                                        }}
                                                    </td>
                                                </tr>
                                            }
                                        }).collect::<Vec<_>>().into_any()
                                    }
                                }}
                            </tbody>
                        </table>
                    </div>
                </div>
            </div>
        </div>
    }
}

#[component]
fn SdkDocView() -> impl IntoView {
    let active_tab = RwSignal::new(0); // 0: Quickstart, 1: Events, 2: Schedules, 3: Status/Logs, 4: Envs CRUD, 5: Full Example

    view! {
        <div class="p-8 space-y-6 max-w-6xl mx-auto">
            // Header
            <div class="flex items-center justify-between pb-4 border-b border-slate-200">
                <div>
                    <div class="flex items-center gap-2 mb-1">
                        <span class="px-2 py-0.5 bg-indigo-100 text-indigo-700 text-xs font-bold rounded">"Client Library"</span>
                        <span class="text-xs text-slate-400 font-mono">"v0.1.0"</span>
                    </div>
                    <h1 class="text-2xl font-bold text-slate-900">"Doopack SDK (Rust) - Documentação Oficial"</h1>
                    <p class="text-sm text-slate-500">"Biblioteca oficial em Rust para aplicações de terceiros se conectarem, publicarem eventos, agendarem jobs e gerenciarem o Doopack."</p>
                </div>
            </div>

            // Installation Card
            <div class="bg-white border border-slate-200 rounded-xl p-6 shadow-sm space-y-4">
                <h3 class="text-base font-bold text-slate-800 flex items-center gap-2">
                    <svg class="w-5 h-5 text-indigo-600" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10" />
                    </svg>
                    "1. Instalação no Cargo.toml"
                </h3>
                <p class="text-sm text-slate-600">"Adicione o pacote ao seu projeto Rust. Ele requer " <code class="font-mono bg-slate-100 text-indigo-600 px-1 py-0.5 rounded text-xs">"tokio"</code> " e " <code class="font-mono bg-slate-100 text-indigo-600 px-1 py-0.5 rounded text-xs">"serde_json"</code> ":"</p>
                <pre class="bg-slate-900 text-slate-100 p-4 rounded-lg font-mono text-xs overflow-x-auto">
{r#"[dependencies]
doopack-sdk = { path = "path/to/doopack/sdks/doopack-sdk" }
tokio = { version = "1.0", features = ["full"] }
serde_json = "1.0""#}
                </pre>
            </div>

            // Navigation Tabs
            <div class="flex border-b border-slate-200 gap-2 overflow-x-auto pb-px">
                <button
                    class=move || format!("px-4 py-2.5 text-xs font-bold border-b-2 transition whitespace-nowrap {}", if active_tab.get() == 0 { "border-indigo-600 text-indigo-600 bg-indigo-50/50 rounded-t-lg" } else { "border-transparent text-slate-500 hover:text-slate-900" })
                    on:click=move |_| active_tab.set(0)
                >
                    "⚡ Inicialização do Cliente"
                </button>
                <button
                    class=move || format!("px-4 py-2.5 text-xs font-bold border-b-2 transition whitespace-nowrap {}", if active_tab.get() == 1 { "border-indigo-600 text-indigo-600 bg-indigo-50/50 rounded-t-lg" } else { "border-transparent text-slate-500 hover:text-slate-900" })
                    on:click=move |_| active_tab.set(1)
                >
                    "a) Publicar Evento (Stream)"
                </button>
                <button
                    class=move || format!("px-4 py-2.5 text-xs font-bold border-b-2 transition whitespace-nowrap {}", if active_tab.get() == 2 { "border-indigo-600 text-indigo-600 bg-indigo-50/50 rounded-t-lg" } else { "border-transparent text-slate-500 hover:text-slate-900" })
                    on:click=move |_| active_tab.set(2)
                >
                    "b) Agendar Evento (Schedule)"
                </button>
                <button
                    class=move || format!("px-4 py-2.5 text-xs font-bold border-b-2 transition whitespace-nowrap {}", if active_tab.get() == 3 { "border-indigo-600 text-indigo-600 bg-indigo-50/50 rounded-t-lg" } else { "border-transparent text-slate-500 hover:text-slate-900" })
                    on:click=move |_| active_tab.set(3)
                >
                    "c) Status & Logs de Execução"
                </button>
                <button
                    class=move || format!("px-4 py-2.5 text-xs font-bold border-b-2 transition whitespace-nowrap {}", if active_tab.get() == 4 { "border-indigo-600 text-indigo-600 bg-indigo-50/50 rounded-t-lg" } else { "border-transparent text-slate-500 hover:text-slate-900" })
                    on:click=move |_| active_tab.set(4)
                >
                    "d) CRUD de Variáveis (Envs)"
                </button>
                <button
                    class=move || format!("px-4 py-2.5 text-xs font-bold border-b-2 transition whitespace-nowrap {}", if active_tab.get() == 5 { "border-indigo-600 text-indigo-600 bg-indigo-50/50 rounded-t-lg" } else { "border-transparent text-slate-500 hover:text-slate-900" })
                    on:click=move |_| active_tab.set(5)
                >
                    "📄 Exemplo Completo"
                </button>
            </div>

            // Tab Content
            {move || match active_tab.get() {
                0 => view! {
                    <div class="space-y-6">
                        <div class="bg-white border border-slate-200 rounded-xl p-6 shadow-sm space-y-4">
                            <h3 class="text-lg font-bold text-slate-800">"Inicialização e Autenticação"</h3>
                            <p class="text-sm text-slate-600 leading-relaxed">
                                "O cliente Doopack suporta autenticação via token JWT (" <code class="font-mono text-xs bg-slate-100 text-indigo-600 px-1 py-0.5 rounded">"Authorization: Bearer"</code> ") ou API Key (" <code class="font-mono text-xs bg-slate-100 text-indigo-600 px-1 py-0.5 rounded">"x-api-key"</code> "). Você também pode carregar as variáveis automaticamente do ambiente."
                            </p>
                            <div class="space-y-3">
                                <h4 class="text-xs font-bold text-slate-700 uppercase tracking-wider">"Opção A: Inicialização Manual"</h4>
                                <pre class="bg-slate-900 text-slate-100 p-4 rounded-lg font-mono text-xs overflow-x-auto">
{r#"use doopack_sdk::DoopackClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Com Token JWT:
    let client = DoopackClient::new("http://localhost:4500")
        .with_token("meu_token_jwt");

    // Ou com API Key:
    let client = DoopackClient::new("http://localhost:4500")
        .with_api_key("sua_chave_api_aqui");

    Ok(())
}"#}
                                </pre>
                            </div>
                            <div class="space-y-3">
                                <h4 class="text-xs font-bold text-slate-700 uppercase tracking-wider">"Opção B: A partir de Variáveis de Ambiente"</h4>
                                <p class="text-sm text-slate-600">"Defina " <code class="font-mono text-xs bg-slate-100 px-1 py-0.5 rounded">"DOOPACK_ENDPOINT"</code> " e " <code class="font-mono text-xs bg-slate-100 px-1 py-0.5 rounded">"DOOPACK_TOKEN"</code> " (ou " <code class="font-mono text-xs bg-slate-100 px-1 py-0.5 rounded">"DOOPACK_API_KEY"</code> "):"</p>
                                <pre class="bg-slate-900 text-slate-100 p-4 rounded-lg font-mono text-xs overflow-x-auto">
{r#"// Lê DOOPACK_ENDPOINT e DOOPACK_TOKEN/DOOPACK_API_KEY automaticamente:
let client = DoopackClient::from_env()?;"#}
                                </pre>
                            </div>
                        </div>
                    </div>
                }.into_any(),

                1 => view! {
                    <div class="space-y-6">
                        <div class="bg-white border border-slate-200 rounded-xl p-6 shadow-sm space-y-4">
                            <div class="flex items-center gap-2">
                                <span class="px-2 py-0.5 bg-emerald-100 text-emerald-800 text-xs font-bold rounded">"a) Publicar Evento"</span>
                                <h3 class="text-lg font-bold text-slate-800">"Publicando Mensagens em Filas / Streams Redis"</h3>
                            </div>
                            <p class="text-sm text-slate-600 leading-relaxed">
                                "Publique payloads JSON diretamente na fila configurada no Doopack. A fila distribuirá automaticamente o evento para os microsserviços vinculados (Bindings) e executará o pipeline configurado."
                            </p>
                            <pre class="bg-slate-900 text-slate-100 p-4 rounded-lg font-mono text-xs overflow-x-auto">
{r#"use serde_json::json;

// Publica um payload no stream 'orders_created'
let response = client.publish("orders_created", &json!({
    "order_id": "ORD-12345",
    "customer_id": 992,
    "amount": 499.90,
    "items": [
        { "sku": "DOOPACK-PRO", "qty": 1 }
    ]
})).await?;

println!("Status: {}", response.status); // "published"
println!("Redis Message ID: {:?}", response.message_id); // ex: "1725182000123-0""#}
                            </pre>
                            <div class="bg-indigo-50 border border-indigo-100 rounded-lg p-4 text-xs text-indigo-900 space-y-1">
                                <span class="font-bold block">"💡 Dica de Integração"</span>
                                <p>"O método " <code class="font-mono font-bold">"client.publish(...)"</code> " aguarda a confirmação de escrita no Redis Stream e retorna o identificador único da mensagem gerada."</p>
                            </div>
                        </div>
                    </div>
                }.into_any(),

                2 => view! {
                    <div class="space-y-6">
                        <div class="bg-white border border-slate-200 rounded-xl p-6 shadow-sm space-y-4">
                            <div class="flex items-center gap-2">
                                <span class="px-2 py-0.5 bg-indigo-100 text-indigo-800 text-xs font-bold rounded">"b) Agendar Evento"</span>
                                <h3 class="text-lg font-bold text-slate-800">"Agendamento de Jobs (Delay, Data e Cron)"</h3>
                            </div>
                            <p class="text-sm text-slate-600 leading-relaxed">
                                "O Doopack suporta 3 modalidades de agendamento: Delay em segundos, Data/Hora específica, ou Expressões Cron recorrentes."
                            </p>
                            <pre class="bg-slate-900 text-slate-100 p-4 rounded-lg font-mono text-xs overflow-x-auto">
{r#"use serde_json::json;

// 1. Agendar com delay em segundos (ex: rodar daqui 30 segundos)
let job_delay = client.schedule_with_delay("email-service", 30, &json!({
    "action": "send_welcome_email",
    "email": "user@example.com"
})).await?;
println!("Job agendado ID: {:?}", job_delay.id);

// 2. Agendar com expressão Cron recorrente (ex: todo dia às 08:00 AM)
let cron_job = client.schedule_cron("billing-service", "0 8 * * *", &json!({
    "task": "daily_invoices"
})).await?;

// 3. Listar agendamentos ativos
let schedules = client.list_schedules().await?;
for s in schedules {
    println!("ID: {:?} | Status: {} | Run At: {}", s.id, s.status, s.run_at);
}

// 4. Cancelar / Deletar agendamento
if let Some(id) = job_delay.id {
    client.delete_schedule(&id).await?;
    println!("Agendamento cancelado!");
}"#}
                            </pre>
                        </div>
                    </div>
                }.into_any(),

                3 => view! {
                    <div class="space-y-6">
                        <div class="bg-white border border-slate-200 rounded-xl p-6 shadow-sm space-y-4">
                            <div class="flex items-center gap-2">
                                <span class="px-2 py-0.5 bg-amber-100 text-amber-800 text-xs font-bold rounded">"c) Status & Logs"</span>
                                <h3 class="text-lg font-bold text-slate-800">"Consulta de Status e Logs de Execução"</h3>
                            </div>
                            <p class="text-sm text-slate-600 leading-relaxed">
                                "Consulte o resultado de execução de qualquer evento processado pelos microsserviços, veja a duração em milissegundos, payload de retorno ou reenvie eventos com falha."
                            </p>
                            <pre class="bg-slate-900 text-slate-100 p-4 rounded-lg font-mono text-xs overflow-x-auto">
{r#"use shared::LogFilterQuery;

// 1. Consultar status e payload de um log específico pelo ID
let log = client.get_event_status("105").await?;
println!("Status da execução: {}", log.status); // "success" | "error" | "timeout" | "panic"
println!("Tempo de execução: {}ms", log.execution_time_ms);
println!("Payload retornado: {:?}", log.payload_output);

// 2. Filtrar logs avançados
let filter = LogFilterQuery {
    microservice_id: Some("orders-processor".to_string()),
    status: Some("error".to_string()),
    ..Default::default()
};
let logs = client.search_logs(&filter).await?;
println!("Encontrados {} logs com erro", logs.len());

// 3. Re-enviar / Re-executar um log
client.resend_log("105").await?;
println!("Log reenviado para reprocessamento!");"#}
                            </pre>
                        </div>
                    </div>
                }.into_any(),

                4 => view! {
                    <div class="space-y-6">
                        <div class="bg-white border border-slate-200 rounded-xl p-6 shadow-sm space-y-4">
                            <div class="flex items-center gap-2">
                                <span class="px-2 py-0.5 bg-purple-100 text-purple-800 text-xs font-bold rounded">"d) CRUD de Envs"</span>
                                <h3 class="text-lg font-bold text-slate-800">"Gerenciamento Programático de Ambientes & Variáveis"</h3>
                            </div>
                            <p class="text-sm text-slate-600 leading-relaxed">
                                "Crie, consulte, edite e remova conjuntos de variáveis de ambiente associados a cada microsserviço de forma 100% automatizada."
                            </p>
                            <pre class="bg-slate-900 text-slate-100 p-4 rounded-lg font-mono text-xs overflow-x-auto">
{r#"use serde_json::json;

let service_id = "orders-processor";

// 1. Listar ambientes existentes
let envs = client.list_envs(service_id).await?;

// 2. Criar um novo ambiente
let new_env = client.create_env(
    service_id,
    "staging_v2",
    &json!({
        "DB_HOST": "postgres://user:pass@staging-db:5432/app",
        "API_TIMEOUT": "30",
        "DEBUG_LOGS": "true"
    }),
    false // is_default
).await?;
let env_id = new_env.id.unwrap();

// 3. Consultar ambiente criado
let env = client.get_env(service_id, &env_id).await?;
println!("Nome do ambiente: {}", env.name);

// 4. Atualizar ambiente
let updated = client.update_env(
    service_id,
    &env_id,
    "staging_v2_updated",
    &json!({ "DEBUG_LOGS": "false" }),
    true // torna padrão
).await?;

// 5. Deletar ambiente
client.delete_env(service_id, &env_id).await?;
println!("Ambiente removido!");"#}
                            </pre>
                        </div>
                    </div>
                }.into_any(),

                5 => view! {
                    <div class="space-y-6">
                        <div class="bg-white border border-slate-200 rounded-xl p-6 shadow-sm space-y-4">
                            <h3 class="text-lg font-bold text-slate-800">"Exemplo Completo de Uso (usage.rs)"</h3>
                            <pre class="bg-slate-900 text-slate-100 p-4 rounded-lg font-mono text-xs overflow-x-auto">
{r#"use doopack_sdk::DoopackClient;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Conectando ao Doopack SDK ===");
    let client = DoopackClient::new("http://localhost:4500")
        .with_token("seu_token_aqui");

    // 1. Publicar Evento
    println!("\n[1] Publicando evento na fila 'payment_events'...");
    let pub_res = client.publish("payment_events", &json!({
        "payment_id": "PAY-8819",
        "status": "APPROVED",
        "amount": 99.90
    })).await?;
    println!("Mensagem publicada com sucesso! ID: {:?}", pub_res.message_id);

    // 2. Agendar Evento
    println!("\n[2] Agendando job com delay de 60 segundos...");
    let job = client.schedule_with_delay("orders-service", 60, &json!({
        "action": "check_status"
    })).await?;
    println!("Job agendado ID: {:?}", job.id);

    // 3. Consultar Status de Log
    println!("\n[3] Consultando status de execução...");
    if let Ok(log) = client.get_event_status("1").await {
        println!("Status: {} | Tempo: {}ms", log.status, log.execution_time_ms);
    }

    // 4. CRUD de Envs
    println!("\n[4] Criando ambiente de configuração...");
    let env = client.create_env(
        "orders-service",
        "production_env",
        &json!({ "ENV": "production", "PORT": 8080 }),
        true
    ).await?;
    println!("Ambiente criado: {:?}", env.id);

    println!("\n=== Todas as operações executadas com sucesso! ===");
    Ok(())
}"#}
                            </pre>
                        </div>
                    </div>
                }.into_any(),

                _ => view! { <div></div> }.into_any(),
            }}
        </div>
    }
}

#[component]
fn BackupView() -> impl IntoView {
    let is_exporting = RwSignal::new(false);
    let is_importing = RwSignal::new(false);
    let status_message = RwSignal::new(None::<(bool, String)>);

    let do_export = move || {
        is_exporting.set(true);
        spawn_local(async move {
            match api::export_system_data().await {
                Ok(json_val) => {
                    if let Some(window) = web_sys::window() {
                        if let Some(document) = window.document() {
                            let json_str = serde_json::to_string_pretty(&json_val).unwrap_or_default();
                            let bag = web_sys::BlobPropertyBag::new();
                            bag.set_type("application/json");
                            let array = js_sys::Array::new();
                            array.push(&wasm_bindgen::JsValue::from_str(&json_str));
                            if let Ok(blob) = web_sys::Blob::new_with_str_sequence_and_options(&array, &bag) {
                                if let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) {
                                    if let Ok(a) = document.create_element("a") {
                                        let a_el: web_sys::HtmlAnchorElement = a.unchecked_into();
                                        a_el.set_href(&url);
                                        let date_str = js_sys::Date::new_0().to_iso_string().as_string().unwrap_or_else(|| "backup".to_string());
                                        a_el.set_download(&format!("doopack_backup_{}.json", date_str.replace(":", "-")));
                                        a_el.click();
                                        let _ = web_sys::Url::revoke_object_url(&url);
                                        status_message.set(Some((true, "Backup exportado com sucesso!".to_string())));
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    status_message.set(Some((false, format!("Falha ao exportar backup: {}", e))));
                }
            }
            is_exporting.set(false);
        });
    };

    view! {
        <div class="p-8 space-y-6 max-w-5xl mx-auto">
            // Header
            <div class="flex items-center justify-between pb-4 border-b border-slate-200">
                <div>
                    <h1 class="text-2xl font-bold text-slate-900">"Backup & Restore Hub"</h1>
                    <p class="text-sm text-slate-500">"Exporte ou importe a configuração completa do ecossistema Doopack com 1 clique."</p>
                </div>
            </div>

            {move || status_message.get().map(|(success, msg)| {
                let bg_class = if success { "bg-emerald-50 border-emerald-200 text-emerald-800" } else { "bg-red-50 border-red-200 text-red-800" };
                view! {
                    <div class=format!("p-4 rounded-xl border text-sm font-medium flex items-center justify-between {}", bg_class)>
                        <span>{msg}</span>
                        <button class="text-xs font-bold underline ml-4" on:click=move |_| status_message.set(None)>"Fechar"</button>
                    </div>
                }
            })}

            <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
                // 1. Export Card
                <div class="bg-white border border-slate-200 rounded-xl p-6 shadow-sm flex flex-col justify-between space-y-6">
                    <div class="space-y-3">
                        <div class="w-10 h-10 bg-indigo-50 text-indigo-600 rounded-lg flex items-center justify-center">
                            <svg class="w-6 h-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
                            </svg>
                        </div>
                        <h3 class="text-lg font-bold text-slate-900">"Exportar Backup Completo"</h3>
                        <p class="text-sm text-slate-600 leading-relaxed">
                            "Gera um arquivo JSON contendo todos os dados do sistema:"
                        </p>
                        <ul class="text-xs text-slate-500 space-y-1 list-disc list-inside">
                            <li>"Microsserviços e Versões/Código"</li>
                            <li>"Filas (Redis Streams)"</li>
                            <li>"Vínculos de Eventos (Bindings)"</li>
                            <li>"Pools de Conexão (DB Pools)"</li>
                            <li>"Variáveis de Ambiente (Envs)"</li>
                            <li>"Agendamentos de Jobs (Schedules)"</li>
                        </ul>
                    </div>

                    <button
                        class="w-full py-3 bg-slate-950 hover:bg-black text-white text-sm font-semibold rounded-lg transition flex items-center justify-center gap-2 shadow-sm active:translate-y-px disabled:opacity-50"
                        disabled=move || is_exporting.get()
                        on:click=move |_| do_export()
                    >
                        <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
                        </svg>
                        {move || if is_exporting.get() { "Exportando arquivo JSON..." } else { "Exportar Agora (.json)" }}
                    </button>
                </div>

                // 2. Import Card
                <div class="bg-white border border-slate-200 rounded-xl p-6 shadow-sm flex flex-col justify-between space-y-6">
                    <div class="space-y-3">
                        <div class="w-10 h-10 bg-emerald-50 text-emerald-600 rounded-lg flex items-center justify-center">
                            <svg class="w-6 h-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12" />
                            </svg>
                        </div>
                        <h3 class="text-lg font-bold text-slate-900">"Restaurar Backup"</h3>
                        <p class="text-sm text-slate-600 leading-relaxed">
                            "Importe um arquivo de snapshot anterior do Doopack. Todos os recursos serão sincronizados e restaurados no banco de dados."
                        </p>
                        <div class="p-3 bg-amber-50 border border-amber-200 rounded-lg text-xs text-amber-800">
                            <span class="font-bold block mb-0.5">"⚠️ Atenção:"</span>
                            "Os itens presentes no backup substituirão registros com o mesmo ID ou nome único existente."
                        </div>
                    </div>

                    <label class="w-full py-3 bg-emerald-600 hover:bg-emerald-700 text-white text-sm font-semibold rounded-lg transition flex items-center justify-center gap-2 shadow-sm cursor-pointer active:translate-y-px">
                        <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12" />
                        </svg>
                        {move || if is_importing.get() { "Importando dados..." } else { "Selecionar Arquivo de Backup (.json)" }}
                        <input
                            type="file"
                            accept=".json"
                            class="hidden"
                            on:change=move |ev| {
                                is_importing.set(true);
                                let file_input: web_sys::HtmlInputElement = ev.target().unwrap().unchecked_into();
                                if let Some(files) = file_input.files() {
                                    if let Some(file) = files.get(0) {
                                        let reader = web_sys::FileReader::new().unwrap();
                                        let reader_c = reader.clone();
                                        let onload = Closure::<dyn FnMut()>::new(move || {
                                            let result = reader_c.result().unwrap();
                                            let text = result.as_string().unwrap_or_default();
                                            if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&text) {
                                                spawn_local(async move {
                                                    match api::import_system_data(json_val).await {
                                                        Ok(_) => {
                                                            let _ = web_sys::window().unwrap().alert_with_message("Backup restaurado com sucesso! Recarregando sistema...");
                                                            if let Some(w) = web_sys::window() {
                                                                let loc = w.location();
                                                                let _ = loc.reload();
                                                            }
                                                        }
                                                        Err(e) => {
                                                            status_message.set(Some((false, format!("Erro ao importar: {}", e))));
                                                        }
                                                    }
                                                    is_importing.set(false);
                                                });
                                            } else {
                                                status_message.set(Some((false, "Arquivo JSON de backup inválido".to_string())));
                                                is_importing.set(false);
                                            }
                                        });
                                        reader.set_onload(Some(onload.as_ref().unchecked_ref()));
                                        let _ = reader.read_as_text(&file);
                                        onload.forget();
                                    }
                                }
                            }
                        />
                    </label>
                </div>
            </div>
        </div>
    }
}
