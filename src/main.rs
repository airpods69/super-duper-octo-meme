#[macro_use] extern crate rocket;
use rocket::serde::{json::Json, Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(crate = "rocket::serde")]
struct Message {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    function_call: Option<FunctionCall>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(crate = "rocket::serde")]
struct FunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(crate = "rocket::serde")]
struct Function {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
struct ChatRequest {
    messages: Vec<Message>,
}

#[derive(Debug, Serialize)]
struct DeepSeekRequest {
    model: String,
    messages: Vec<Message>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    functions: Option<Vec<Function>>,
}

#[derive(Debug, Deserialize)]
struct DeepSeekChoice {
    message: Message,
}

#[derive(Debug, Deserialize)]
struct DeepSeekResponse {
    choices: Vec<DeepSeekChoice>,
}

async fn call_deepseek(messages: Vec<Message>, functions: Option<Vec<Function>>) -> Result<String, reqwest::Error> {
    let api_key = std::env::var("DEEPSEEK_API_KEY")
        .expect("DEEPSEEK_API_KEY must be set in environment");

    let client = reqwest::Client::new();
    let request = DeepSeekRequest {
        model: "deepseek-chat".to_string(),
        messages,
        stream: false,
        functions,
    };

    println!("Sending request to DeepSeek API: {:?}", request);

    let response = client
        .post("https://api.deepseek.com/chat/completions")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&request)
        .send()
        .await?;

    let status = response.status();
    let headers = response.headers().clone();
    let response_text = response.text().await?;

    println!("DeepSeek API response status: {}", status);
    println!("DeepSeek API response body: {}", response_text);

    Ok(response_text)
}

async fn search_duckduckgo(query: &str) -> Result<String, Box<dyn std::error::Error>> {
    use scraper::{Html, Selector};
    use std::time::Duration;

    // First, get search results from DuckDuckGo
    let search_url = format!("https://html.duckduckgo.com/html/?q={}", query);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;
    let response = client.get(&search_url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36")
        .send()
        .await?;
    let html = response.text().await?;
    
    // Extract result URLs in a separate block to drop document before await
    let urls = {
        let document = Html::parse_document(&html);
        let selector = Selector::parse(".result__url").unwrap();
        let mut urls = Vec::new();
        for element in document.select(&selector) {
            if let Some(href) = element.value().attr("href") {
                let url = if href.starts_with("//") {
                    format!("https:{}", href)
                } else {
                    href.to_string()
                };
                urls.push(url);
            }
        }
        urls
    };
    
    // Take top 5 URLs
    let urls = urls.into_iter().take(5).collect::<Vec<_>>();
    let mut combined_content = String::new();
    
    // Scrape content from each URL
    for url in urls {
        match client.get(&url).send().await {
            Ok(response) => {
                if let Ok(html) = response.text().await {
                    let doc = Html::parse_document(&html);
                    let body_selector = Selector::parse("body").unwrap();
                    if let Some(body) = doc.select(&body_selector).next() {
                        let text = body.text().collect::<Vec<_>>().join(" ");
                        combined_content.push_str(&format!("URL: {}\nContent: {}\n\n", url, text));
                    }
                }
            }
            Err(e) => {
                combined_content.push_str(&format!("Failed to fetch {}: {}\n", url, e));
            }
        }
    }
    
    if combined_content.is_empty() {
        combined_content = "No content found".to_string();
    }
    
    Ok(combined_content)
}

#[post("/create_plan", data = "<request>")]
async fn create_plan(request: Json<ChatRequest>) -> String {
    let system_message = Message {
        role: "system".to_string(),
        content: Some("You are a technical planning AI agent with web search capabilities. You help users create technical plans for their ideas, including the architecture. You can search the web using DuckDuckGo to gather information. Use the search_web function when needed. You can perform up to 20 searches per plan. After gathering information, create a comprehensive technical plan.".to_string()),
        name: None,
        function_call: None,
    };

    let mut new_messages = vec![system_message];
    for msg in &request.messages {
        new_messages.push(Message {
            role: msg.role.clone(),
            content: msg.content.clone(),
            name: None,
            function_call: None,
        });
    }

    let search_function = Function {
        name: "search_web".to_string(),
        description: "Search the web using DuckDuckGo to gather information for technical planning".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query to use for DuckDuckGo",
                }
            },
            "required": ["query"],
        }),
    };

    let mut search_count = 0;
    let max_searches = 20;

    loop {
        println!("Sending messages to DeepSeek: {:?}", new_messages);
        let response = match call_deepseek(new_messages.clone(), Some(vec![search_function.clone()])).await {
            Ok(response) => response,
            Err(e) => return format!("Error calling DeepSeek API: {}", e),
        };

        let parsed: DeepSeekResponse = match serde_json::from_str(&response) {
            Ok(parsed) => parsed,
            Err(e) => return format!("Error parsing DeepSeek response: {}", e),
        };

        if let Some(choice) = parsed.choices.get(0) {
            let message = &choice.message;
            if let Some(function_call) = &message.function_call {
                if function_call.name == "search_web" && search_count < max_searches {
                    search_count += 1;
                    println!("Performing search #{}: {}", search_count, function_call.arguments);
                    
                    let args: serde_json::Value = match serde_json::from_str(&function_call.arguments) {
                        Ok(args) => args,
                        Err(e) => return format!("Error parsing function arguments: {}", e),
                    };
                    
                    let query = match args["query"].as_str() {
                        Some(query) => query,
                        None => return "Error: missing query in search function".to_string(),
                    };
                    
                    let search_result = match search_duckduckgo(query).await {
                        Ok(result) => result,
                        Err(e) => format!("Search error: {}", e),
                    };
        
                    // Store useful information from this search
                    new_messages.push(Message {
                        role: "system".to_string(),
                        content: Some(format!("Useful information from search #{} for '{}':\n{}", search_count, query, search_result)),
                        name: None,
                        function_call: None,
                    });
                    
                    new_messages.push(Message {
                        role: "assistant".to_string(),
                        content: None,
                        name: None,
                        function_call: Some(function_call.clone()),
                    });
                    
                    new_messages.push(Message {
                        role: "function".to_string(),
                        content: Some(search_result),
                        name: Some("search_web".to_string()),
                        function_call: None,
                    });
                    continue;
                }
            }
            
            if let Some(content) = &message.content {
                return content.clone();
            }
        }
        
        return "Error: No valid response from DeepSeek".to_string();
    }
}

#[post("/chat", data = "<request>")]
async fn chat(request: Json<ChatRequest>) -> String {
    println!("Received messages: {:?}", request.messages);
    let messages: Vec<Message> = request.messages.iter().map(|msg| Message {
        role: msg.role.clone(),
        content: msg.content.clone(),
        name: None,
        function_call: None,
    }).collect();
    
    match call_deepseek(messages, None).await {
        Ok(response) => {
            match serde_json::from_str::<DeepSeekResponse>(&response) {
                Ok(parsed) => {
                    if let Some(content) = &parsed.choices[0].message.content {
                        content.clone()
                    } else {
                        "Error: No content in response".to_string()
                    }
                },
                Err(e) => format!("Error parsing DeepSeek response: {}", e),
            }
        }
        Err(e) => format!("Error calling DeepSeek API: {}", e),
    }
}

#[launch]
fn rocket() -> _ {
    rocket::build().mount("/planner", routes![chat, create_plan])
}
