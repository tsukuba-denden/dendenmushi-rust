
use std::fmt::Display;

use openai_dive::v1::resources::{response::{request::ResponseParametersBuilder, response::ResponseReasoning}, shared::ReasoningEffort};

#[derive(Clone)]
pub struct Config {
    pub discord_token: String,
    pub openai_api_key: String,
    pub system_prompt: String,
    pub rale_limit_window_size: u64,
    pub rate_limit_sec_per_cost: u64,
}

impl Config {
    pub fn new() -> Self {
        let discord_token =
            std::env::var("DISCORD_TOKEN").expect("DISCORD_TOKEN must be set");
        let openai_api_key =
            std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY must be set");
        let system_prompt =
            std::env::var("SYSTEM_PROMPT").unwrap_or_else(|_| "あなたはDiscordのObserverとして自然に会話
できるだけ並列にtool
知識的なことは情報源の確認や最新の情報かの確認、内容をでっちあげないように調べまくること。
完璧な応答を目指し、必要ならだけ質問
情報元を明示
論理的思考と時事調査を重視、足りない部分はtoolを使用
並列toolを使いこなすこと。ネットでしらべても局所的な情報である可能性を考えてしっかりしらべること！
重要: 検索するときとかは今の日時がわかるように、大体いつも把握できるように定期的にget_timeをtool_callの前に使おう
$explainは会話中の言語で書くこと かかなくてもいいけど
自分にあまり自身を持ちすぎないこと、あいまいな情報は調べるように
toolは並列に使おう。とくにweb検索とか
tool call の結果は他人に見えないのでちゃんと書き出そうね
あまり'!'やemojiを使わないようにしてください
discord系ツールは使うときに必ず一度提案するように。適切な場面で提案してくれたらうれしいな
discord-tool-reactionは楽しいので積極的に使おう
一語だけで応答が可能な場合は、一語で応答してる（例: 'うん', 'そう', 'は？', 'えぇ...', '???', 'にゃん'）
バランスのとれた、知的に引き込む、かつ技術的に情報に精通した態度を保で会話を続けている
カジュアルに会話する。会話の流れを意識している。敬語は使うべき時だけ使う。
重要: スラングなどを適度に使い、返答は端的でわかりやすくする(1文だけなど。)
静かでおとなしく、論理的な性格で会話してる 一人称は基本私
重要: 周りの人の話し方などを真似て口調を調節する".to_string());
        Config {
            discord_token,
            openai_api_key,
            system_prompt,
            rale_limit_window_size: 7200,
            rate_limit_sec_per_cost: 900,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Models {
    Gpt5Mini,
    Gpt5Nano,
    Gpt5dot1,
    O4Mini,
    O3,
    Gpt5dot1CodexMini
}

impl Into<String> for Models {
    fn into(self) -> String {
        match self {
            Models::Gpt5Mini => "gpt-5-mini".to_string(),
            Models::Gpt5Nano => "gpt-5-nano".to_string(),
            Models::Gpt5dot1 => "gpt-5.1".to_string(),
            Models::O4Mini => "o4-mini".to_string(),
            Models::O3 => "o3".to_string(),
            Models::Gpt5dot1CodexMini => "gpt-5.1-codex-mini".to_string(),
        }
    }
}

impl Display for Models {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let model_str: String = self.clone().into();
        write!(f, "{}", model_str)
    }
}

impl From<String> for Models {
    fn from(s: String) -> Models {
        match s.as_str() {
            "gpt-5-mini" => Models::Gpt5Mini,
            "gpt-5-nano" => Models::Gpt5Nano,
            "gpt-5.1" => Models::Gpt5dot1,
            "o4-mini" => Models::O4Mini,
            "o3" => Models::O3,
            "gpt-5.1-codex-mini" => Models::Gpt5dot1CodexMini,
            _ => Models::Gpt5Nano, // default
        }
    }
}

impl Models {
    pub fn list() -> Vec<Models> {
        vec![
            Models::Gpt5Mini,
            Models::Gpt5Nano,
            Models::Gpt5dot1,
            Models::O4Mini,
            Models::O3,
            Models::Gpt5dot1CodexMini
        ]
    }

    pub fn rate_cost(&self) -> u64 {
        match self {
            Models::Gpt5Mini => 1,
            Models::Gpt5Nano => 2,
            Models::Gpt5dot1 => 6,
            Models::O4Mini => 3,
            Models::O3 => 6,
            Models::Gpt5dot1CodexMini => 2,
        }
    }

    pub fn to_parameter(&self) -> ResponseParametersBuilder {
        match self {
            Models::Gpt5Mini => {
                ResponseParametersBuilder::default().model("gpt-5-mini")
                .reasoning(
                    ResponseReasoning {
                        effort: Some(ReasoningEffort::Low),
                    }
                ).clone()
            }
            Models::Gpt5Nano => {
                ResponseParametersBuilder::default().model("gpt-5-nano")
                .reasoning(
                    ResponseReasoning {
                        effort: Some(ReasoningEffort::Low),
                    }
                ).clone()
            }
            Models::Gpt5dot1 => { 
                ResponseParametersBuilder::default().model("gpt-5.1")
                .reasoning(
                    ResponseReasoning {
                        effort: Some(ReasoningEffort::Low),
                    }
                ).clone()
            }
            Models::O4Mini => { 
                ResponseParametersBuilder::default().model("o4-mini")
                .reasoning(
                    ResponseReasoning {
                        effort: Some(ReasoningEffort::Low),
                    }
                ).clone()
            }
            Models::O3 => { 
                ResponseParametersBuilder::default().model("o3")
                .reasoning(
                    ResponseReasoning {
                        effort: Some(ReasoningEffort::Low),
                    }
                ).clone()
            }
            Models::Gpt5dot1CodexMini => { 
                ResponseParametersBuilder::default().model("gpt-5.1-codex-mini")
                .reasoning(
                    ResponseReasoning {
                        effort: Some(ReasoningEffort::Low),
                    }
                ).clone()
            }
        }
    }
}