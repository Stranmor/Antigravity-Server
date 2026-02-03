//! JSON Schema cleaning entry points and utility functions
//!
//! Functions for preparing JSON Schema for Gemini API compatibility.

use once_cell::sync::Lazy;
use serde_json::Value;

use super::super::tool_adapter::ToolAdapter;
use super::super::tool_adapters::PencilAdapter;
use super::recursive::clean_json_schema_recursive;

static TOOL_ADAPTERS: Lazy<Vec<Box<dyn ToolAdapter>>> = Lazy::new(|| vec![Box::new(PencilAdapter)]);

pub fn clean_json_schema_for_tool(value: &mut Value, tool_name: &str) {
    let adapter = TOOL_ADAPTERS.iter().find(|a| a.matches(tool_name));

    if let Some(adapter) = adapter {
        let _ = adapter.pre_process(value);
    }

    clean_json_schema(value);

    if let Some(adapter) = adapter {
        let _ = adapter.post_process(value);
    }
}

/// 递归清理 JSON Schema 以符合 Gemini 接口要求
///
/// 1. [New] 展开 $ref 和 $defs: 将引用替换为实际定义，解决 Gemini 不支持 $ref 的问题
/// 2. 移除不支持的字段: $schema, additionalProperties, format, default, uniqueItems, validation fields
/// 3. 处理联合类型: ["string", "null"] -> "string"
/// 4. [NEW] 处理 anyOf 联合类型: anyOf: [{"type": "string"}, {"type": "null"}] -> "type": "string"
/// 5. 将 type 字段的值转换为小写 (Gemini v1internal 要求)
/// 6. 移除数字校验字段: multipleOf, exclusiveMinimum, exclusiveMaximum 等
pub fn clean_json_schema(value: &mut Value) {
    // 0. 预处理：展开 $ref (Schema Flattening)
    // [FIX #952] 递归收集所有层级的 $defs/definitions，而非仅从根层级提取
    let mut all_defs = serde_json::Map::new();
    collect_all_defs(value, &mut all_defs);

    // 移除根层级的 $defs/definitions (保持向后兼容)
    if let Value::Object(map) = value {
        map.remove("$defs");
        map.remove("definitions");
    }

    // [FIX #952] 始终运行 flatten_refs，即使 defs 为空
    // 这样可以捕获并处理无法解析的 $ref (降级为 string 类型)
    if let Value::Object(map) = value {
        flatten_refs(map, &all_defs);
    }

    // 递归清理
    clean_json_schema_recursive(value);
}

/// [NEW #952] 递归收集所有层级的 $defs 和 definitions
///
/// MCP 工具的 schema 可能在任意嵌套层级定义 $defs，而非仅在根层级。
/// 此函数深度遍历整个 schema，收集所有定义到统一的 map 中。
fn collect_all_defs(value: &Value, defs: &mut serde_json::Map<String, Value>) {
    if let Value::Object(map) = value {
        // 收集当前层级的 $defs
        if let Some(Value::Object(d)) = map.get("$defs") {
            for (k, v) in d {
                // 避免覆盖已存在的定义（先定义的优先）
                defs.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
        // 收集当前层级的 definitions (Draft-07 风格)
        if let Some(Value::Object(d)) = map.get("definitions") {
            for (k, v) in d {
                defs.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
        // 递归处理所有子节点
        for (key, v) in map {
            // 跳过 $defs/definitions 本身，避免重复处理
            if key != "$defs" && key != "definitions" {
                collect_all_defs(v, defs);
            }
        }
    } else if let Value::Array(arr) = value {
        for item in arr {
            collect_all_defs(item, defs);
        }
    }
}

/// 递归展开 $ref
fn flatten_refs(map: &mut serde_json::Map<String, Value>, defs: &serde_json::Map<String, Value>) {
    // 检查并替换 $ref
    if let Some(Value::String(ref_path)) = map.remove("$ref") {
        // 解析引用名 (例如 #/$defs/MyType -> MyType)
        let ref_name = ref_path.split('/').next_back().unwrap_or(&ref_path);

        if let Some(def_schema) = defs.get(ref_name) {
            // 将定义的内容合并到当前 map
            if let Value::Object(def_map) = def_schema {
                for (k, v) in def_map {
                    // 仅当当前 map 没有该 key 时才插入 (避免覆盖)
                    // 但通常 $ref 节点不应该有其他属性
                    map.entry(k.clone()).or_insert_with(|| v.clone());
                }

                // 递归处理刚刚合并进来的内容中可能包含的 $ref
                // 注意：这里可能会无限递归如果存在循环引用，但工具定义通常是 DAG
                flatten_refs(map, defs);
            }
        } else {
            // [FIX #952] 无法解析的 $ref: 转换为宽松的 string 类型，避免 API 400 错误
            map.insert("type".to_string(), serde_json::json!("string"));
            let hint = format!("(Unresolved $ref: {})", ref_path);
            let desc_val = map
                .entry("description".to_string())
                .or_insert_with(|| Value::String(String::new()));
            if let Value::String(s) = desc_val {
                if !s.contains(&hint) {
                    if !s.is_empty() {
                        s.push(' ');
                    }
                    s.push_str(&hint);
                }
            }
        }
    }

    // 遍历子节点
    for (_, v) in map.iter_mut() {
        if let Value::Object(child_map) = v {
            flatten_refs(child_map, defs);
        } else if let Value::Array(arr) = v {
            for item in arr {
                if let Value::Object(item_map) = item {
                    flatten_refs(item_map, defs);
                }
            }
        }
    }
}
