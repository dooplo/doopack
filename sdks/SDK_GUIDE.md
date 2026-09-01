# Guia de Integração e Uso do SDK no DooPack

Este guia explica como construir microsserviços compatíveis com o ecossistema **DooPack**, detalhando o fluxo de entrada e saída de dados, o gerenciamento de conexões com bancos de dados (como o SurrealDB) e exemplos práticos.

---

## 1. Modelo de Execução do DooPack
No DooPack, os microsserviços são executados como **containers Docker sob demanda (Serverless/Epêmeros)**.
1. O worker monitora uma fila/stream do Redis.
2. Quando uma nova mensagem chega, o worker inicia uma nova instância do container do microsserviço.
3. O payload da mensagem é injetado no container através da variável de ambiente `PAYLOAD_INPUT`.
4. O container executa, processa o payload, imprime o resultado em formato JSON no `stdout` (saída padrão) e finaliza.
5. O worker do DooPack captura o `stdout`, deleta o evento do Redis e engatilha as ações de sucesso ou erro configuradas.

> [!NOTE]
> Como os containers iniciam e morrem a cada mensagem, **não é possível manter um pool de conexões persistente em memória entre execuções diferentes**. Cada execução deve estabelecer sua própria conexão com o banco, executar a query e encerrar.

---

## 2. Entrada e Saída de Dados

### Entrada
O payload enviado pelo Redis estará disponível na variável de ambiente `PAYLOAD_INPUT` em formato de string JSON.

### Saída
Para enviar a resposta com sucesso de volta ao DooPack, basta serializar seu objeto em formato JSON e imprimi-lo no `stdout`. Se o processo terminar com código de saída `0` e imprimir um JSON válido, o DooPack considerará a execução um **Sucesso**.

---

## 3. Exemplo Prático em Rust (usando o SDK)

O SDK do Rust simplifica esse processo com os métodos `get_input()` e `send_output()`.

### Configuração do `Cargo.toml`
Adicione a dependência do SDK local:
```toml
[dependencies]
doopack-sdk = { path = "../../sdks/rust-sdk" }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
surrealdb = { version = "1.0", features = ["kv-mem"] } # Opcional se for conectar ao SurrealDB
tokio = { version = "1.0", features = ["full"] }
```

### Código do Microsserviço (`main.rs`)
```rust
use serde::{Deserialize, Serialize};
use serde_json::json;
use surrealdb::engine::remote::ws::Ws;
use surrealdb::opt::auth::Root;
use surrealdb::Surreal;

#[derive(Deserialize, Debug)]
struct OrderInput {
    id: i64,
    price: f64,
}

#[derive(Serialize, Debug)]
struct OrderOutput {
    order_id: i64,
    status: String,
    invoice_created: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Recuperar o input injetado pelo DooPack
    let input_value = rust_sdk::get_input()?;
    let order: OrderInput = serde_json::from_value(input_value)?;

    // 2. Conectar ao SurrealDB (Exemplo de conexão por execução)
    // As credenciais podem ser passadas via variáveis de ambiente configuradas no container
    let db = Surreal::new::<Ws>("127.0.0.1:8000").await?;
    db.signin(Root {
        username: "root",
        password: "rootpassword",
    }).await?;
    db.use_ns("doopack").use_db("main").await?;

    // Executar queries necessárias
    let query_result: Option<serde_json::Value> = db
        .create(("orders", order.id.to_string()))
        .content(json!({
            "price": order.price,
            "processed_at": chrono::Utc::now()
        }))
        .await?;

    // 3. Enviar a resposta de sucesso de volta para o DooPack
    let output = OrderOutput {
        order_id: order.id,
        status: "processed".to_string(),
        invoice_created: query_result.is_some(),
    };
    
    rust_sdk::send_output(&output);
    Ok(())
}
```

---

## 4. Exemplo Prático em Python (Sem SDK dedicado)

Caso construa microsserviços em outras linguagens, o padrão de integração é idêntico: ler a variável `PAYLOAD_INPUT` e escrever no `stdout`.

```python
import os
import json
import sys

def main():
    try:
        # 1. Ler o payload de entrada
        payload_str = os.environ.get("PAYLOAD_INPUT")
        if not payload_str:
            print(json.dumps({"error": "PAYLOAD_INPUT not found"}), file=sys.stderr)
            sys.exit(1)
            
        payload = json.loads(payload_str)
        order_id = payload.get("id")
        price = payload.get("price")

        # 2. Conexão e processamento de banco de dados
        # Estabelecer conexão local a cada invocação
        # ... executar queries ...

        # 3. Enviar saída padrão formatada em JSON
        output = {
            "order_id": order_id,
            "status": "completed",
            "price_taxed": price * 1.1
        }
        print(json.dumps(output))
        sys.exit(0)
        
    except Exception as e:
        print(json.dumps({"error": str(e)}), file=sys.stderr)
        sys.exit(1)

if __name__ == "__main__":
    main()
```

---

## 5. Gerenciamento de Variáveis de Ambiente (Envs)

### Onde Adicionar as Envs?
Você pode gerenciar as variáveis de ambiente diretamente no modal **Deploy Version**:
1. Vá para a página de **Microservices**.
2. Clique no botão **Deploy** de qualquer microsserviço para abrir o modal de Deploy.
3. No final do modal, há uma seção dedicada chamada **Manage Environment Variables**.
4. Lá, você pode adicionar novas configurações em formato JSON (por exemplo, `{"DATABASE_URL": "mongodb://...", "API_KEY": "xyz"}`), excluir ou definir qual ambiente será o **Default**.

### Como Selecionar o Ambiente no Payload?
Ao enviar uma mensagem para a fila, você pode especificar qual ambiente carregar passando a chave especial `{ "doopack": { "env": "<nome_da_env>" } }` no payload.
Se essa chave não estiver presente ou a env informada não for encontrada, o DooPack carregará as configurações da env marcada como **Default**.

### Exemplo em Rust:
```rust
fn main() {
    // Carrega o payload
    let input = rust_sdk::get_input().unwrap();
    
    // Ler variáveis de ambiente injetadas dinamicamente:
    let api_key = std::env::var("API_KEY").unwrap_or_else(|_| "chave_fallback".to_string());
    println!("API Key carregada: {}", api_key);
}
```

---

## 6. Ações Condicionais por Evento de Chave (Key Events)

Além de encaminhar o output completo de um microsserviço diretamente para uma fila de sucesso ou de erro, você pode configurar condições baseadas no conteúdo JSON retornado.

Ao criar ou editar um microsserviço:
1. Em **On Success Action** ou **On Error Action**, selecione a opção **Key Event Condition**.
2. Preencha os seguintes parâmetros:
   - **Key Path**: O caminho da chave no JSON de retorno (ex: `status` ou `result.code`).
   - **Operator**: O operador de comparação (`==`, `!=`, `>`, `<`).
   - **Value to Compare**: O valor que você espera encontrar para ativar a rota (ex: `200` ou `success`).
   - **Destination Queue**: A fila do Redis para onde o payload será publicado se a comparação for verdadeira.

---

## 7. Integração com a API HTTP Externa e Chaves de API (API Keys)

Para gerenciar recursos do DooPack de forma programática via requisições HTTP externas, você deve utilizar as chaves de API criadas na aba **API Keys** no painel de administração.

### Autenticação nas requisições HTTP
As requisições devem incluir a chave de API em um dos seguintes formatos de cabeçalhos:
1. `X-API-Key: dp_sua_chave_aqui`
2. `Authorization: Bearer dp_sua_chave_aqui`

### Exemplo: Listar Variáveis de Ambiente via cURL
```bash
curl -X GET "http://localhost:4500/api/v1/services/1/envs" \
  -H "X-API-Key: dp_sua_chave_aqui"
```

### Exemplo: Cadastrar/Atualizar Variáveis de Ambiente via cURL
```bash
curl -X POST "http://localhost:4500/api/v1/services/1/envs" \
  -H "X-API-Key: dp_sua_chave_aqui" \
  -H "Content-Type: application/json" \
  -d '{
    "microservice_id": "1",
    "name": "prod",
    "config": {
      "DATABASE_URL": "mongodb://production_host:27017",
      "API_KEY": "prod_secret_key"
    },
    "is_default": true
  }'
```


