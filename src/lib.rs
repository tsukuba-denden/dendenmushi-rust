use std::sync::Arc;
pub mod prefix;
pub mod tools;

use call_agent::chat::{client::{ModelConfig, OpenAIClient}, prompt::{Message, MessageContext}};
use tools::{get_time::GetTime, memory::MemoryTool, web_scraper::WebScraper};


#[tokio::main]
async fn main() {
    let mut c = OpenAIClient::new(
        prefix::settings::model::MAIN_MODEL_ENDPOINT, 
        Some(&prefix::settings::model::MAIN_MODEL_API_KEY));
    //c.def_tool(Arc::new(TextLengthTool::new()));
    c.def_tool(Arc::new(GetTime::new()));
    //c.def_tool(Arc::new(WebSearch::new()));
    c.def_tool(Arc::new(WebScraper::new()));
    c.def_tool(Arc::new(MemoryTool::new()));

    let conf = ModelConfig {
        model: "gpt-4o-mini".to_string(),
        model_name: None,
        parallel_tool_calls: None,
        temperature: Some(0.5),
        max_completion_tokens: Some(4000),
        reasoning_effort: None,
        presence_penalty: Some(0.0),
        strict: Some(false),
        top_p: Some(1.0),
    };

    c.set_model_config(&conf);

    
    let mut prompt_stream = c.create_prompt();
    loop {
        let mut input = String::new();
        std::io::stdin()
            .read_line(&mut input)
            .expect("Failed to read line");
        let prompt = vec![Message::User 
        {
            content:vec![
                MessageContext::Text(input.trim().to_string())
                ], 
            name: Some("371tti".to_string())
        }
        ];
        prompt_stream.add(prompt).await;

        loop {
            // Ask for a continuation or function response
            let r = prompt_stream.generate_can_use_tool(Some(&conf)).await;
            let res = prompt_stream.last().await.unwrap();
            println!("{:?}", res);

            match res {
                // If the response comes from a tool, continue generating.
                Message::Tool { .. } => continue,
                // When we have a plain response, try to extract its text and print it.
                Message::Assistant { ref content, .. } => {
                    if let Some(MessageContext::Text(text)) = content.first() {
                        // Replace escape sequences with actual newlines
                        let formatted_text = text.replace("\\n", "\n");
                        println!("\n\n{}", formatted_text);
                    } else {
                        println!("{:?}", res);
                    }
                    break;
                }
                // Fallback for any other message type.
                _ => {
                    println!("{:?}", res);
                    break;
                }
            }
        }
    }

}
