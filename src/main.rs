#[macro_use] extern crate rocket;
use rocket::serde::{json::Json, Deserialize, Serialize};
use serde_json::{json, Value};

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

#[derive(Debug)]
enum ResearchPhase {
    Foundational,
    ComponentAnalysis(String),
    Synthesis,
}

struct ResearchState {
    phase: ResearchPhase,
    components: Vec<String>,
    knowledge_base: String,
    search_count: usize,
    max_searches: usize,
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

use futures::stream::StreamExt;
use tokio::io::AsyncWriteExt;
use rocket::http::hyper::body::Bytes;

async fn call_deepseek(messages: Vec<Message>, functions: Option<Vec<Function>>) -> Result<String, Box<dyn std::error::Error>> {
    let api_key = std::env::var("DEEPSEEK_API_KEY")
        .expect("DEEPSEEK_API_KEY must be set in environment");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300)) // 5 minute timeout
        .connect_timeout(std::time::Duration::from_secs(30))
        .http1_only()
        .build()?;

    let request = DeepSeekRequest {
        model: "deepseek-chat".to_string(),
        messages,
        stream: true,  // Enable streaming
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
    println!("DeepSeek API response status: {}", status);

    if !status.is_success() {
        let error_text = response.text().await?;
        println!("DeepSeek API error response: {}", error_text);
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("DeepSeek API error: {}", status),
        )));
    }

    let mut response_bytes = Vec::new();
    let mut stream = response.bytes_stream();
    let mut combined_content = String::new();

    while let Some(item) = stream.next().await {
        let chunk: Bytes = item?;
        response_bytes.extend_from_slice(&chunk);

        // Process each chunk for streaming log
        if let Ok(chunk_str) = std::str::from_utf8(&chunk) {
            // Split by Server-Sent Events (SSE) format
            for event in chunk_str.split("\n\n").filter(|s| s.starts_with("data: {")) {
                let json_str = &event[6..]; // Remove "data: " prefix
                if let Ok(event_data) = serde_json::from_str::<serde_json::Value>(json_str) {
                    if let Some(choices) = event_data["choices"].as_array() {
                        for choice in choices {
                            if let Some(delta) = choice["delta"].as_object() {
                                if let Some(content) = delta["content"].as_str() {
                                    // Stream log the content chunk
                                    print!("{}", content);
                                    tokio::io::stdout().flush().await?;
                                    combined_content.push_str(content);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    println!(); // Newline after streaming content
    Ok(combined_content)
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


// TODO: Fix the parser error with JSON
#[post("/create_plan", data = "<request>")]
async fn create_plan(request: Json<ChatRequest>) -> String {
    // ------------------------------------------------------------------
    // 0. Sanity helpers
    // ------------------------------------------------------------------
    let user_goal = request
        .messages
        .first()
        .and_then(|m| m.content.as_deref())
        .unwrap_or("")
        .trim();
    if user_goal.is_empty() {
        return "Error: empty prompt".to_string();
    }

    // ------------------------------------------------------------------
    // 1. QUESTION PHASE (6–7 questions)  -------------------------------
    // ------------------------------------------------------------------
    if request.messages.len() == 1 {
        let system_prompt = r#"
You are **PlanBot**.
Your ONLY job right now is to ask the user **exactly six** crisp, high-impact questions that will let you write a bullet-proof technical plan later.

Rules:
- One question per line, no numbering.
- Do NOT greet or explain.
- Do NOT ask more than six questions.
"#.trim();

        let msgs = vec![
            Message {
                role: "system".to_string(),
                content: Some(system_prompt.to_string()),
                name: None,
                function_call: None,
            },
            Message {
                role: "user".to_string(),
                content: Some(user_goal.to_string()),
                name: None,
                function_call: None,
            },
        ];

        return match call_deepseek(msgs, None).await {
            Ok(content) => content,
            Err(e) => format!("api error: {e}"),
        };
    }

    // ------------------------------------------------------------------
    // 2. RESEARCH PHASE  ------------------------------------------------
    // ------------------------------------------------------------------
    // Current search budget
    const MAX_SEARCHES: usize = 50;
    let mut search_count = 0usize;
    let mut knowledge_base = String::new();

    // Helper: decide if we need another loop
    fn should_continue(count: usize, kb: &str) -> bool {
        count < MAX_SEARCHES
            && (!kb.contains("<<FINAL_ANSWER>>")
                && !kb.contains("## Final Technical Plan"))
    }

    // Kick-off prompt for DeepSeek
    let mut messages = vec![
        Message {
            role: "system".to_string(),
            content: Some(
                r#"
You are **PlanBot-researcher**.
You will be given the user’s goal + answers to your 6 questions.
Your job: iteratively search, analyse, search again until you possess **enough** information to write the final plan.

Workflow inside this loop:
1. Decide what you still need to know.
2. Emit **exactly one** JSON call to function `search_web` with a sharp query.
3. Read the returned snippets.
4. Append a short synthesis to the knowledge base.
5. If satisfied, append "<<FINAL_ANSWER>>" to the knowledge base and exit the loop.
6. Otherwise repeat.

You may perform at most 50 searches.
"#
                .to_string(),
            ),
            name: None,
            function_call: None,
        },
    ];
    messages.extend(request.messages.clone());

    let search_fn = vec![Function {
        name: "search_web".to_string(),
        description: "Search DuckDuckGo".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" }
            },
            "required": ["query"]
        }),
    }];

    while should_continue(search_count, &knowledge_base) {
        let resp_text = match call_deepseek(messages.clone(), Some(search_fn.clone())).await {
            Ok(t) => t,
            Err(e) => return format!("DeepSeek error: {e}"),
        };

        let resp: DeepSeekResponse = match serde_json::from_str(&resp_text) {
            Ok(r) => r,
            Err(e) => return format!("parse error: {e}"),
        };
        let assistant_msg = resp.choices[0].message.clone();

        // Case 1: DeepSeek wants to search
        if let Some(ref fc) = assistant_msg.function_call {
            if fc.name == "search_web" {
                let args: serde_json::Value = serde_json::from_str(&fc.arguments)
                    .unwrap_or_else(|_| serde_json::json!({}));
                let query = args["query"].as_str().unwrap_or("").to_string();
                let search_result = search_duckduckgo(&query).await.unwrap_or_default();
                search_count += 1;

                // Feed the search result back as a function-return message
                messages.push(Message {
                    role: "assistant".to_string(),
                    content: None,
                    name: None,
                    function_call: Some(fc.clone()),
                });
                messages.push(Message {
                    role: "function".to_string(),
                    content: Some(search_result),
                    name: Some("search_web".to_string()),
                    function_call: None,
                });

                knowledge_base.push_str(&format!(
                    "\n--- Search #{search_count}: {query} ---\n"
                ));
                continue;
            }
        }

        // Case 2: DeepSignalled it is done
        if let Some(ref content) = assistant_msg.content {
            knowledge_base.push_str(&content);
            if content.contains("<<FINAL_ANSWER>>") {
                break;
            }
        }

        // Otherwise treat as intermediate synthesis
        messages.push(assistant_msg);
    }

    // ------------------------------------------------------------------
    // 3. PLAN PHASE  ----------------------------------------------------
    // ------------------------------------------------------------------
    let final_prompt = ChatRequest {
        messages: vec![
            Message {
                role: "system".to_string(),
                content: Some(
                    "You are **PlanBot-final**.  \
                    Using the knowledge base below, write a **comprehensive technical plan** \
                    with clear sections, timelines, and deliverables."
                        .to_string(),
                ),
                name: None,
                function_call: None,
            },
            Message {
                role: "user".to_string(),
                content: Some(knowledge_base),
                name: None,
                function_call: None,
            },
        ],
    };

    match call_deepseek(final_prompt.messages, None).await {
        Ok(resp) => {
            serde_json::from_str::<DeepSeekResponse>(&resp)
                .map(|r| r.choices[0].message.content.clone().unwrap_or_default())
                .unwrap_or_else(|e| format!("final parse error: {e}"))
        }
        Err(e) => format!("final api error: {e}"),
    }
}
//
// #[post("/create_plan", data = "<request>")]
// async fn create_plan(request: Json<ChatRequest>) -> String {
//     // First, check if we need to generate questions
//     let is_initial_request = request.messages.len() == 1;
//
//     if is_initial_request {
//         let system_message = Message {
//             role: "system".to_string(),
//             content: Some("You are an expert technical planning assistant. Your task is to ask 8-10 probing questions to thoroughly understand the user's requirements. Consider these aspects:
// 1. Technical constraints and requirements
// 2. Business goals and success metrics
// 3. Target users and their needs
// 4. Integration points with other systems
// 5. Security and compliance considerations
// 6. Performance and scalability needs
// 7. Budget and timeline constraints
// 8. Team skills and resources
//
// Ask clear, specific questions one at a time to gather comprehensive information before planning.".to_string()),
//             name: None,
//             function_call: None,
//         };
//
//         let user_message = Message {
//             role: "user".to_string(),
//             content: request.messages[0].content.clone(),
//             name: None,
//             function_call: None,
//         };
//
//         let messages = vec![system_message, user_message];
//
//         match call_deepseek(messages, None).await {
//             Ok(response) => {
//                 match serde_json::from_str::<DeepSeekResponse>(&response) {
//                     Ok(parsed) => {
//                         if let Some(content) = &parsed.choices[0].message.content {
//                             return content.clone();
//                         } else {
//                             return "Error: No content in response".to_string();
//                         }
//                     },
//                     Err(e) => return format!("Error parsing DeepSeek response: {}", e),
//                 }
//             }
//             Err(e) => return format!("Error calling DeepSeek API: {}", e),
//         }
//     }
//     else {
//         // Proceed with normal planning
//         let system_message = Message {
//             role: "system".to_string(),
//             content: Some("You are an expert technical planning AI. Follow this rigorous process:
// 1. First conduct foundational research to understand core concepts
// 2. Break down the problem into key components
// 3. For each component:
//    - Search for technical specifications
//    - Search for case studies and real-world examples
//    - Search for performance benchmarks
//    - Search for alternative approaches
// 4. Synthesize findings after each research phase
// 5. Continue researching until all aspects are thoroughly understood
// 6. Only then create the final comprehensive plan".to_string()),
//             name: None,
//             function_call: None,
//         };
//
//         let mut new_messages = vec![system_message];
//         for msg in &request.messages {
//             new_messages.push(Message {
//                 role: msg.role.clone(),
//                 content: msg.content.clone(),
//                 name: None,
//                 function_call: None,
//             });
//         }
//
//         let search_function = Function {
//             name: "search_web".to_string(),
//             description: "Search the web using DuckDuckGo to gather information for technical planning".to_string(),
//             parameters: serde_json::json!({
//                 "type": "object",
//                 "properties": {
//                     "query": {
//                         "type": "string",
//                         "description": "The search query to use for DuckDuckGo",
//                     }
//                 },
//                 "required": ["query"],
//             }),
//         };
//
//         let mut research_state = ResearchState {
//             phase: ResearchPhase::Foundational,
//             components: Vec::new(),
//             knowledge_base: String::new(),
//             search_count: 0,
//             max_searches: 50,
//         };
//
//         loop {
//             let query = match research_state.phase {
//                 ResearchPhase::Foundational => {
//                     format!("Foundational research about: {}", request.messages[0].content.as_ref().unwrap_or(&"".to_string()))
//                 }
//                 ResearchPhase::ComponentAnalysis(ref component) => {
//                     format!("Technical details about {} for: {}", component, request.messages[0].content.as_ref().unwrap_or(&"".to_string()))
//                 }
//                 ResearchPhase::Synthesis => break,
//             };
//
//             let search_result = match search_duckduckgo(&query).await {
//                 Ok(result) => result,
//                 Err(e) => format!("Search error: {}", e),
//             };
//
//             research_state.knowledge_base.push_str(&format!("\n\n## Research for '{}':\n{}", query, search_result));
//             research_state.search_count += 1;
//
//             // Analyze results and determine next steps
//             let analysis_request = ChatRequest {
//                 messages: vec![
//                     Message {
//                         role: "system".to_string(),
//                         content: Some(format!("Analyze this research and determine next steps:\nCurrent Phase: {:?}\nKnowledge So Far:\n{}", research_state.phase, research_state.knowledge_base)),
//                         name: None,
//                         function_call: None,
//                     }
//                 ]
//             };
//
//             let analysis = match call_deepseek(analysis_request.messages, None).await {
//                 Ok(response) => response,
//                 Err(e) => return format!("Error analyzing research: {}", e),
//             };
//
//             // Update research state based on analysis
//             if analysis.contains("sufficient foundational") {
//                 research_state.phase = ResearchPhase::ComponentAnalysis("Technical Specifications".to_string().clone());
//             } else if analysis.contains("all components researched") {
//                 research_state.phase = ResearchPhase::Synthesis;
//             }
//         }
//
//         // Final plan generation
//         let plan_request = ChatRequest {
//             messages: vec![
//                 Message {
//                     role: "system".to_string(),
//                     content: Some("Generate comprehensive technical plan based on this research:".to_string()),
//                     name: None,
//                     function_call: None,
//                 },
//                 Message {
//                     role: "user".to_string(),
//                     content: Some(research_state.knowledge_base),
//                     name: None,
//                     function_call: None,
//                 }
//             ]
//         };
//
//         match call_deepseek(plan_request.messages, None).await {
//             Ok(response) => {
//                 match serde_json::from_str::<DeepSeekResponse>(&response) {
//                     Ok(parsed) => {
//                         if let Some(content) = &parsed.choices[0].message.content {
//                             content.clone()
//                         } else {
//                             "Error: No content in response".to_string()
//                         }
//                     },
//                     Err(e) => format!("Error parsing DeepSeek response: {}", e),
//                 }
//             }
//             Err(e) => format!("Error generating final plan: {}", e),
//         }
//     }
// }

#[post("/chat", data = "<request>")]
async fn chat(request: Json<ChatRequest>) -> Json<Value> {
    println!("Received messages: {:?}", request.messages);
    let messages: Vec<Message> = request.messages.iter().map(|msg| Message {
        role: msg.role.clone(),
        content: msg.content.clone(),
        name: None,
        function_call: None,
    }).collect();

    match call_deepseek(messages, None).await {
        Ok(content) => Json(json!({ "content": content })),
        Err(e) => Json(json!({ "error": format!("Error calling DeepSeek API: {}", e) })),
    }
}

use clap::{Parser, Subcommand};
use rocket::Config;
use std::net::Ipv4Addr;
use std::io::{self, Write};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    mode: Mode,
}

#[derive(Subcommand, Debug)]
enum Mode {
    /// Start the web server
    Server,
    /// Run in CLI mode
    Cli,
}

async fn run_cli() -> io::Result<()> {
    println!("Welcome to MLS GigaChad CLI Mode!");
    println!("Type your messages below (type 'exit' or 'quit' to end)");
    println!("------------------------------------------------------");
    println!("Note: Press Ctrl+C to cancel any operation | Ctrl+L to clear screen");

    let mut messages = Vec::new();

    loop {
        print!("> ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input.eq_ignore_ascii_case("exit") || input.eq_ignore_ascii_case("quit") ||
           input.eq_ignore_ascii_case("/q") || input.eq_ignore_ascii_case("/quit") {
            println!("Goodbye!");
            break;
        }

        if input.is_empty() {
            continue;
        }

        // Add user message
        messages.push(Message {
            role: "user".to_string(),
            content: Some(input.to_string()),
            name: None,
            function_call: None,
        });

        // Ask if user wants chat or plan
        'mode_choice: loop {
            println!("Choose mode: (c)hat, (p)lan, (b)ack to re-enter message, (r)eset context");
            print!("[c/p/b/r]> ");
            io::stdout().flush()?;
            let mut choice = String::new();
            io::stdin().read_line(&mut choice)?;
            let choice = choice.trim().to_lowercase();

            match choice.as_str() {
                "p" => {
                    println!("\nCreating plan... (this may take a moment)");
                    println!("Press Ctrl+C to cancel the operation");

                    let request = ChatRequest { messages: messages.clone() };
                    let response = tokio::select! {
                        response = create_plan(Json(request)) => response,
                        _ = tokio::signal::ctrl_c() => {
                            println!("\nOperation cancelled by user.");
                            continue 'mode_choice;
                        }
                    };

                    println!("\nAssistant: {}", response);
                    println!("----------------------------\n");

                    // Add assistant response to context
                    messages.push(Message {
                        role: "assistant".to_string(),
                        content: Some(response),
                        name: None,
                        function_call: None,
                    });
                    break 'mode_choice;
                }
                "c" => {
                    println!("\nChatting...");
                    println!("Press Ctrl+C to cancel the operation");

                    let request = ChatRequest { messages: messages.clone() };
                    let response = tokio::select! {
                        response = chat(Json(request)) => response,
                        _ = tokio::signal::ctrl_c() => {
                            println!("\nOperation cancelled by user.");
                            continue 'mode_choice;
                        }
                    };

                    let response_content = match response.into_inner() {
                        Value::Object(mut obj) => {
                            if let Some(Value::String(content)) = obj.remove("content") {
                                content
                            } else if let Some(Value::String(err)) = obj.remove("error") {
                                format!("Error: {}", err)
                            } else {
                                "Unexpected response format".to_string()
                            }
                        }
                        _ => "Unexpected response format".to_string(),
                    };

                    println!("\nAssistant: {}", response_content);
                    println!("----------------------------\n");

                    // Add assistant response to context
                    messages.push(Message {
                        role: "assistant".to_string(),
                        content: Some(response_content),
                        name: None,
                        function_call: None,
                    });
                    break 'mode_choice;
                }
                "b" => {
                    messages.pop(); // Remove last message
                    println!("Message discarded. Enter new message:");
                    break 'mode_choice;
                }
                "r" => {
                    messages.clear();
                    println!("Context reset. Starting fresh conversation.");
                    break 'mode_choice;
                }
                _ => {
                    println!("Invalid choice. Please enter 'c', 'p', 'b', or 'r'");
                    continue;
                }
            }
        }
    }

    Ok(())
}

use rocket::fs::{FileServer, relative};

#[rocket::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    match args.mode {
        Mode::Server => {
            println!("Starting MLS GigaChad Web Server...");
            println!("API Endpoints:");
            println!("- POST http://localhost:8000/planner/chat");
            println!("- POST http://localhost:8000/planner/create_plan");
            println!("\nServer running on http://localhost:8000");
            println!("Press CTRL+C to stop\n");

            let config = Config {
                port: 8000,
                address: Ipv4Addr::new(0, 0, 0, 0).into(),
                keep_alive: 300, // 5 minutes
                ..Config::default()
            };

            rocket::build()
                .configure(config)
                .mount("/", FileServer::from(relative!("static")))
                .mount("/planner", routes![chat, create_plan])
                .launch()
                .await?;
        }
        Mode::Cli => {
            run_cli().await?;
        }
    }

    Ok(())
}
