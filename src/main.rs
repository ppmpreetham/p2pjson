use anyhow::{Context, Result};
use iroh::{
    endpoint::Connection,
    protocol::{ProtocolHandler, Router},
    Endpoint, NodeAddr,
};
use serde::{Deserialize, Serialize};
use std::{env, io, str::FromStr};
use tokio::io::AsyncWriteExt;

// Protocol identifier
const ALPN: &[u8] = b"iroh-json-transfer/0";

// Example JSON structure
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Message {
    sender: String,
    content: String,
    timestamp: String,
    metadata: Option<Metadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Metadata {
    message_type: String,
    priority: u8,
    tags: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        println!("Usage:");
        println!("  server - Start as a server");
        println!("  client <address> - Connect as a client to the given address");
        return Ok(());
    }

    match args[1].as_str() {
        "server" => {
            println!("Starting as server...");
            let router = start_server().await?;
            let node_addr = router.endpoint().node_addr().await?;
            println!("Server started with address: {}", node_addr.node_id);
            
            // Keep running until user presses Enter
            println!("Press Enter to shutdown server");
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            
            println!("Shutting down server...");
            router.shutdown().await?;
        },
        "client" => {
            if args.len() < 3 {
                println!("Please provide a server address");
                return Ok(());
            }
            
            let node_id = args[2].parse().context("Failed to parse node ID")?;
            let addr = NodeAddr::new(node_id);              
            println!("Starting as client, connecting to {:?}", addr);
            run_client(addr).await?;
        },
        _ => {
            println!("Unknown command: {}", args[1]);
            println!("Use 'server' or 'client <address>'");
        }
    }
    
    Ok(())
}

// Helper function to get current timestamp
fn get_timestamp() -> String {
    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

// Helper function to get current user
fn get_user() -> String {
    env::var("USER").unwrap_or_else(|_| "ppm".to_string())
}

// Server implementation
async fn start_server() -> Result<Router> {
    let endpoint = Endpoint::builder().discovery_n0().bind().await?;
    println!("Server endpoint bound successfully");
    
    // Build protocol handler and router
    let router = Router::builder(endpoint).accept(ALPN, JsonReceiver).spawn();
    
    Ok(router)
}

#[derive(Debug, Clone)]
struct JsonReceiver;

impl ProtocolHandler for JsonReceiver {
    fn accept(&self, connection: Connection) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>> {
        Box::pin(async move {
            let node_id = connection.remote_node_id()?;
            println!("Server: Accepted connection from {node_id}");
            
            // Handle bidirectional stream
            println!("Server: Waiting for bidirectional stream...");
            let (mut bi_send, mut bi_recv) = connection.accept_bi().await?;
            
            // Read the JSON data
            let bi_data = bi_recv.read_to_end(1024 * 1024).await?;
            let bi_json_str = String::from_utf8(bi_data)?;
            
            // Parse the JSON
            let received_message: Message = serde_json::from_str(&bi_json_str)?;
            println!("Server: Received message: {:#?}", received_message);
            
            // Create a response message
            let response = Message {
                sender: get_user(),
                content: "I received your message!".to_string(),
                timestamp: get_timestamp(),
                metadata: Some(Metadata {
                    message_type: "response".to_string(),
                    priority: 1,
                    tags: vec!["acknowledgement".to_string()],
                }),
            };
            
            // Serialize and send response
            let response_json = serde_json::to_string(&response)?;
            bi_send.write_all(response_json.as_bytes()).await?;
            bi_send.finish()?;
            
            // Wait for connection to close
            println!("Server: Waiting for connection to close...");
            connection.closed().await;
            println!("Server: Connection closed");
            
            Ok(())
        })
    }
}

// Client implementation
async fn run_client(server_addr: NodeAddr) -> Result<()> {
    println!("Client: Connecting to server at {:?}", server_addr);
    
    let endpoint = Endpoint::builder().discovery_n0().bind().await?;
    
    // Connect to the server
    let conn = endpoint.connect(server_addr, ALPN).await?;
    println!("Client: Connected to server");
    
    // Bidirectional communication with JSON
    println!("Client: Opening bidirectional stream");
    let (mut bi_send, mut bi_recv) = conn.open_bi().await?;
    
    // Create a message
    let message = Message {
        sender: get_user(),
        content: "Hello, this is a test message!".to_string(),
        timestamp: get_timestamp(),
        metadata: Some(Metadata {
            message_type: "greeting".to_string(),
            priority: 1,
            tags: vec!["hello".to_string(), "test".to_string()],
        }),
    };
    
    // Get JSON for the message
    let json_data = getJSON(&message)?;
    println!("Client: Sending message: {}", String::from_utf8_lossy(&json_data));
    
    // Send the message
    bi_send.write_all(&json_data).await?;
    bi_send.finish()?;
    
    // Wait for response
    println!("Client: Waiting for response...");
    let response_data = bi_recv.read_to_end(1024 * 1024).await?;
    
    // Parse the response
    let response: Message = serde_json::from_slice(&response_data)?;
    println!("Client: Received response: {:#?}", response);
    
    // Close connection gracefully
    println!("Client: Closing connection");
    conn.close(0u32.into(), b"done");
    endpoint.close().await;
    
    Ok(())
}

// This is the getJSON function mentioned in the requirements
fn getJSON<T: Serialize>(data: &T) -> Result<Vec<u8>> {
    let json = serde_json::to_vec(data)?;
    Ok(json)
}