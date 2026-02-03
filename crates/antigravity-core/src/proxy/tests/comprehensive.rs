#[cfg(test)]
mod tests {
    use crate::proxy::mappers::claude::models::{
        ClaudeRequest, ContentBlock, Message, MessageContent, ThinkingConfig,
    };
    use crate::proxy::mappers::claude::request::transform_claude_request_in;
    use crate::proxy::mappers::claude::thinking_utils::{
        analyze_conversation_state, close_tool_loop_for_thinking,
    };
    use serde_json::json;

    // ==================================================================================
    // scenario一：首times Thinking request (P0-2 Fix)
    // verifyatdoes not havehistory签name 情况下，首times发give Thinking requestwhetherbe放行 (Perimssive Mode)
    // ==================================================================================
    #[test]
    fn test_first_thinking_request_permissive_mode() {
        // 1. 构造a全新 request (无historymessage)
        let req = ClaudeRequest {
            model: "claude-3-7-sonnet-20250219".to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: MessageContent::String("Hello, please think.".to_string()),
            }],
            system: None,
            tools: None, // 无toolcall
            stream: false,
            max_tokens: None,
            temperature: None,
            top_p: None,
            top_k: None,
            thinking: Some(ThinkingConfig {
                type_: "enabled".to_string(),
                budget_tokens: Some(1024),
            }),
            metadata: None,
            output_config: None,
        };

        // 2. executeconvert
        // ifrepair生效，hereshouldsuccessreturn，and thinkingConfig bepreserve
        let result = transform_claude_request_in(&req, "test-project", false);
        assert!(result.is_ok(), "First thinking request should be allowed");

        let body = result.unwrap();
        let request = &body["request"];

        // verify thinkingConfig whetherexist (即 thinking modenot yetbedisable)
        let has_thinking_config = request
            .get("generationConfig")
            .and_then(|g| g.get("thinkingConfig"))
            .is_some();

        assert!(
            has_thinking_config,
            "Thinking config should be preserved for first request without tool calls"
        );
    }

    // ==================================================================================
    // scenario二：toolcircularrecover (P1-4 Fix)
    // verifywhenhistorymessagein丢失 Thinking blockcause死circularwhen，whetherwillautoinject合成message来闭环
    // ==================================================================================
    #[test]
    fn test_tool_loop_recovery() {
        // 1. 构造a "Broken Tool Loop" scenario
        // Assistant (ToolUse) -> User (ToolResult)
        // but Assistant messageinMissing  Thinking block (simulatebe stripping)
        let mut messages = vec![
            Message {
                role: "user".to_string(),
                content: MessageContent::String("Check weather".to_string()),
            },
            Message {
                role: "assistant".to_string(),
                content: MessageContent::Array(vec![
                    // 只have ToolUse，does not have Thinking (Broken State)
                    ContentBlock::ToolUse {
                        id: "call_1".to_string(),
                        name: "get_weather".to_string(),
                        input: json!({"location": "Beijing"}),
                        signature: None,
                        cache_control: None,
                    },
                ]),
            },
            Message {
                role: "user".to_string(),
                content: MessageContent::Array(vec![ContentBlock::ToolResult {
                    tool_use_id: "call_1".to_string(),
                    content: json!("Sunny"),
                    is_error: None,
                }]),
            },
        ];

        // 2. 分析currentstatus
        let state = analyze_conversation_state(&messages);
        assert!(state.in_tool_loop, "Should detect tool loop");

        // 3. executerecoverlogic
        close_tool_loop_for_thinking(&mut messages);

        // 4. verifywhetherinject合成message
        assert_eq!(
            messages.len(),
            5,
            "Should have injected 2 synthetic messages"
        );

        // verify倒数第二条is Assistant   "Completed" message
        let injected_assistant = &messages[3];
        assert_eq!(injected_assistant.role, "assistant");

        // verify最after一条is User   "Proceed" message
        let injected_user = &messages[4];
        assert_eq!(injected_user.role, "user");

        // this waycurrentstatus就not再is "in_tool_loop" (最after一条is User Text)，modelcanstart新  Thinking
        let new_state = analyze_conversation_state(&messages);
        assert!(!new_state.in_tool_loop, "Tool loop should be broken/closed");
    }

    // ==================================================================================
    // scenario三：跨modelcompatible性 (P1-5 Fix) - simulate
    // by于 request.rs in  is_model_compatible is私have ，wevia集成testverify效果
    // ==================================================================================
    /*
       note：by于 is_model_compatible  and cachelogicdepth集成at transform_claude_request_in in，
       anddependglobalsingleton SignatureCache，元test较难simulate "cache旧签namebut切换model"  status。
       heremainlyviaverify "notcompatible签namebe丢弃"  副作用（即 thoughtSignature fieldmessage）来test。
       butby于 SignatureCache isglobal ，wecannot intestin轻易pre-置status。
       因this，thisscenariomainlydepend Verification Guide in 手动test。
       or者，wecantest request.rs in公开 某些 helper (ifhave 话)，but目beforedoes not have。
    */
}
