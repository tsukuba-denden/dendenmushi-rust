use std::collections::VecDeque;

use openai_dive::v1::{api::Client, endpoints::models::Models, models::{Gpt4Model, Gpt5Model}, resources::response::{items::Message, request::{ResponseInput, ResponseInputItem, ResponseParametersBuilder}, response::ResponseOutput, shared::ResponseTool}};

pub struct LMClient {
    pub client: Client,
}

impl LMClient {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    pub async fn generate_response(
        &self,
        context: &LMContext,
        max_tokens: Option<u32>,
        tools: Vec<ResponseTool>,
    ) -> openai_dive::v1::api::resources::response::Response {
        let mut req = self.client.responses().create(
            ResponseParametersBuilder::default()
            .model(Gpt5Model::Gpt5Mini.to_string())
            .input(ResponseInput::List(context.buf.into()))
            .max_output_tokens(max_tokens.unwrap_or(100))
            .tools(tools)
            .parallel_tool_calls(true)
            .build().unwrap(),
        ).await.unwrap();

        let tool_call_request: Option<ResponseOutput> = None;
        for i in 0..10 {
            let parameters = ResponseParametersBuilder::default()
                .model(Gpt5Model::Gpt5Mini.to_string())
                .input(ResponseInput::List(context.buf.into()))
                .max_output_tokens(max_tokens.unwrap_or(100))
                .tools(tools)
                .parallel_tool_calls(true)
                .build().unwrap();

            let result = self.client.responses().create(parameters).await.unwrap();

        }
    }
}

pub struct LMContext {
    pub buf: VecDeque<ResponseInputItem>
}

trait LMTool {
    fn define() -> ResponseTool;
} 