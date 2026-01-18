use serde_json::Value;

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
    if let Value::Object(map) = value {
        let mut defs = serde_json::Map::new();
        // 提取 $defs 或 definitions
        if let Some(Value::Object(d)) = map.remove("$defs") {
            defs.extend(d);
        }
        if let Some(Value::Object(d)) = map.remove("definitions") {
            defs.extend(d);
        }

        if !defs.is_empty() {
            // 递归替换引用
            flatten_refs(map, &defs);
        }
    }

    // 递归清理
    clean_json_schema_recursive(value);
}

/// 递归展开 $ref
fn flatten_refs(map: &mut serde_json::Map<String, Value>, defs: &serde_json::Map<String, Value>) {
    // 检查并替换 $ref
    if let Some(Value::String(ref_path)) = map.remove("$ref") {
        // 解析引用名 (例如 #/$defs/MyType -> MyType)
        let ref_name = ref_path.split('/').last().unwrap_or(&ref_path);

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

fn clean_json_schema_recursive(value: &mut Value) -> bool {
    let mut is_effectively_nullable = false;

    match value {
        Value::Object(map) => {
            // 0. [NEW] 合并 allOf
            merge_all_of(map);

            // 1. [CRITICAL] 深度递归处理子项
            if let Some(Value::Object(props)) = map.get_mut("properties") {
                let mut nullable_keys = std::collections::HashSet::new();
                for (k, v) in props {
                    if clean_json_schema_recursive(v) {
                        nullable_keys.insert(k.clone());
                    }
                }

                if !nullable_keys.is_empty() {
                    if let Some(Value::Array(req_arr)) = map.get_mut("required") {
                        req_arr.retain(|r| {
                            r.as_str().map(|s| !nullable_keys.contains(s)).unwrap_or(true)
                        });
                        if req_arr.is_empty() {
                            map.remove("required");
                        }
                    }
                }
            } else if let Some(items) = map.get_mut("items") {
                clean_json_schema_recursive(items);
            } else {
                for v in map.values_mut() {
                    clean_json_schema_recursive(v);
                }
            }

            // 2. [FIX #815] 处理 anyOf/oneOf 联合类型: 合并属性而非直接删除
            let mut union_to_merge = None;
            if map.get("type").is_none() || map.get("type").and_then(|t| t.as_str()) == Some("object") {
                if let Some(Value::Array(any_of)) = map.get("anyOf") {
                    union_to_merge = Some(any_of.clone());
                } else if let Some(Value::Array(one_of)) = map.get("oneOf") {
                    union_to_merge = Some(one_of.clone());
                }
            }

            if let Some(union_array) = union_to_merge {
                if let Some(best_branch) = extract_best_schema_from_union(&union_array) {
                    if let Value::Object(branch_obj) = best_branch {
                        for (k, v) in branch_obj {
                            if k == "properties" {
                                if let Some(target_props) = map.entry("properties".to_string()).or_insert_with(|| Value::Object(serde_json::Map::new())).as_object_mut() {
                                    if let Some(source_props) = v.as_object() {
                                        for (pk, pv) in source_props {
                                            target_props.entry(pk.clone()).or_insert_with(|| pv.clone());
                                        }
                                    }
                                }
                            } else if k == "required" {
                                if let Some(target_req) = map.entry("required".to_string()).or_insert_with(|| Value::Array(Vec::new())).as_array_mut() {
                                    if let Some(source_req) = v.as_array() {
                                        for rv in source_req {
                                            if !target_req.contains(rv) {
                                                target_req.push(rv.clone());
                                            }
                                        }
                                    }
                                }
                            } else if !map.contains_key(&k) {
                                map.insert(k, v);
                            }
                        }
                    }
                }
            }

            // 3. [SAFETY] 检查当前对象是否为 JSON Schema 节点
            // 只有当对象看起来像 Schema (包含 type, properties, items, enum, anyOf 等) 时，才执行白名单过滤。
            // 否则，如果它是一个普通的 Value (如 request.rs 中的 functionCall 对象)，直接应用激进过滤会破坏结构。
            let looks_like_schema = map.contains_key("type")
                || map.contains_key("properties")
                || map.contains_key("items")
                || map.contains_key("enum")
                || map.contains_key("anyOf")
                || map.contains_key("oneOf")
                || map.contains_key("allOf");

            if looks_like_schema {
                // 4. [ROBUST] 约束迁移：在被白名单过滤前，将校验项转为描述 Hint
                let mut hints = Vec::new();
                let constraints = [
                    ("minLength", "minLen"),
                    ("maxLength", "maxLen"),
                    ("pattern", "pattern"),
                    ("minimum", "min"),
                    ("maximum", "max"),
                    ("multipleOf", "multipleOf"),
                    ("exclusiveMinimum", "exclMin"),
                    ("exclusiveMaximum", "exclMax"),
                    ("minItems", "minItems"),
                    ("maxItems", "maxItems"),
                    ("propertyNames", "propertyNames"),
                    ("format", "format"),
                ];
                for (field, label) in constraints {
                    if let Some(val) = map.get(field) {
                        if !val.is_null() {
                            let val_str = if let Some(s) = val.as_str() { s.to_string() } else { val.to_string() };
                            hints.push(format!("{}: {}", label, val_str));
                        }
                    }
                }
                if !hints.is_empty() {
                    let suffix = format!(" [Constraint: {}]", hints.join(", "));
                    let desc_val = map.entry("description".to_string()).or_insert_with(|| Value::String("".to_string()));
                    if let Value::String(s) = desc_val {
                        if !s.contains(&suffix) { s.push_str(&suffix); }
                    }
                }

                // 5. [CRITICAL] 白名单过滤：彻底物理移除 Gemini 不支持的内容，防止 400 错误
                let allowed_fields = std::collections::HashSet::from([
                    "type", "description", "properties", "required", "items", "enum", "title"
                ]);
                let keys_to_remove: Vec<String> = map.keys()
                    .filter(|k| !allowed_fields.contains(k.as_str()))
                    .cloned()
                    .collect();
                for k in keys_to_remove {
                    map.remove(&k);
                }

                // 6. [SAFETY] 处理空 Object
                if map.get("type").and_then(|t| t.as_str()) == Some("object") {
                    let has_props = map.get("properties").and_then(|p| p.as_object()).map(|o| !o.is_empty()).unwrap_or(false);
                    if !has_props {
                        map.insert("properties".to_string(), serde_json::json!({
                            "reason": { "type": "string", "description": "Reason for calling this tool" }
                        }));
                        map.insert("required".to_string(), serde_json::json!(["reason"]));
                    }
                }

                // 7. [SAFETY] Required 字段对齐
                let valid_prop_keys: Option<std::collections::HashSet<String>> = map
                    .get("properties")
                    .and_then(|p| p.as_object())
                    .map(|obj| obj.keys().cloned().collect());

                if let Some(required_val) = map.get_mut("required") {
                    if let Some(req_arr) = required_val.as_array_mut() {
                        if let Some(keys) = &valid_prop_keys {
                            req_arr.retain(|k| k.as_str().map(|s| keys.contains(s)).unwrap_or(false));
                        } else {
                            req_arr.clear();
                        }
                    }
                }

                // 8. 处理 type 字段
                if let Some(type_val) = map.get_mut("type") {
                    let mut selected_type = None;
                    match type_val {
                        Value::String(s) => {
                            let lower = s.to_lowercase();
                            if lower == "null" { is_effectively_nullable = true; }
                            else { selected_type = Some(lower); }
                        }
                        Value::Array(arr) => {
                            for item in arr {
                                if let Value::String(s) = item {
                                    let lower = s.to_lowercase();
                                    if lower == "null" { is_effectively_nullable = true; }
                                    else if selected_type.is_none() { selected_type = Some(lower); }
                                }
                            }
                        }
                        _ => {}
                    }
                    *type_val = Value::String(selected_type.unwrap_or_else(|| "string".to_string()));
                }

                if is_effectively_nullable {
                    let desc_val = map.entry("description".to_string()).or_insert_with(|| Value::String("".to_string()));
                    if let Value::String(s) = desc_val {
                        if !s.contains("nullable") {
                            if !s.is_empty() { s.push(' '); }
                            s.push_str("(nullable)");
                        }
                    }
                }

                // 9. Enum 值强制转字符串
                if let Some(Value::Array(arr)) = map.get_mut("enum") {
                    for item in arr {
                        if !item.is_string() {
                            *item = Value::String(if item.is_null() { "null".to_string() } else { item.to_string() });
                        }
                    }
                }
            }
        }
        _ => {}
    }

    is_effectively_nullable
}

/// [NEW] 合并 allOf 数组中的所有子 Schema
fn merge_all_of(map: &mut serde_json::Map<String, Value>) {
    if let Some(Value::Array(all_of)) = map.remove("allOf") {
        let mut merged_properties = serde_json::Map::new();
        let mut merged_required = std::collections::HashSet::new();
        let mut other_fields = serde_json::Map::new();

        for sub_schema in all_of {
            if let Value::Object(sub_map) = sub_schema {
                // 合并属性
                if let Some(Value::Object(props)) = sub_map.get("properties") {
                    for (k, v) in props {
                        merged_properties.insert(k.clone(), v.clone());
                    }
                }

                // 合并 required
                if let Some(Value::Array(reqs)) = sub_map.get("required") {
                    for req in reqs {
                        if let Some(s) = req.as_str() {
                            merged_required.insert(s.to_string());
                        }
                    }
                }

                // 合并其余字段 (第一个出现的胜出)
                for (k, v) in sub_map {
                    if k != "properties" && k != "required" && k != "allOf" && !other_fields.contains_key(&k) {
                        other_fields.insert(k, v);
                    }
                }
            }
        }

        // 应用合并后的字段
        for (k, v) in other_fields {
            if !map.contains_key(&k) {
                map.insert(k, v);
            }
        }

        if !merged_properties.is_empty() {
            let existing_props = map.entry("properties".to_string()).or_insert_with(|| Value::Object(serde_json::Map::new()));
            if let Value::Object(existing_map) = existing_props {
                for (k, v) in merged_properties {
                    existing_map.entry(k).or_insert(v);
                }
            }
        }

        if !merged_required.is_empty() {
            let existing_reqs = map.entry("required".to_string()).or_insert_with(|| Value::Array(Vec::new()));
            if let Value::Array(req_arr) = existing_reqs {
                let mut current_reqs: std::collections::HashSet<String> = req_arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                for req in merged_required {
                    if current_reqs.insert(req.clone()) {
                        req_arr.push(Value::String(req));
                    }
                }
            }
        }
    }
}

/// [NEW] 计算 Schema 分支的复杂度得分 (用于 anyOf/oneOf 择优)
/// 评分标准: Object (3) > Array (2) > Scalar (1) > Null (0)
fn score_schema_option(val: &Value) -> i32 {
    if let Value::Object(obj) = val {
        if obj.contains_key("properties") || obj.get("type").and_then(|t| t.as_str()) == Some("object") {
            return 3;
        }
        if obj.contains_key("items") || obj.get("type").and_then(|t| t.as_str()) == Some("array") {
            return 2;
        }
        if let Some(type_str) = obj.get("type").and_then(|t| t.as_str()) {
            if type_str != "null" {
                return 1;
            }
        }
    }
    0
}

/// [NEW] 从 anyOf/oneOf 联合类型数组中选取最佳非 null Schema 分支
fn extract_best_schema_from_union(union_array: &Vec<Value>) -> Option<Value> {
    let mut best_option: Option<&Value> = None;
    let mut best_score = -1;

    for item in union_array {
        let score = score_schema_option(item);
        if score > best_score {
            best_score = score;
            best_option = Some(item);
        }
    }

    best_option.cloned()
}


#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_clean_json_schema_draft_2020_12() {
        let mut schema = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "location": {
                    "type": "string",
                    "minLength": 1,
                    "format": "city"
                },
                // 模拟属性名冲突：pattern 是一个 Object 属性，不应被移除
                "pattern": {
                    "type": "object",
                    "properties": {
                        "regex": { "type": "string", "pattern": "^[a-z]+$" }
                    }
                },
                "unit": {
                    "type": ["string", "null"],
                    "default": "celsius"
                }
            },
            "required": ["location"]
        });

        clean_json_schema(&mut schema);

        // 1. 验证类型保持小写
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["properties"]["location"]["type"], "string");

        // 2. 验证标准字段被移除并转为描述 (Robust Constraint Migration)
        assert!(schema["properties"]["location"].get("minLength").is_none());
        assert!(schema["properties"]["location"].get("format").is_none());
        assert!(schema["properties"]["location"]["description"]
            .as_str()
            .unwrap()
            .contains("[Constraint: minLen: 1, format: city]"));

        // 3. 验证名为 "pattern" 的属性未被误删
        assert!(schema["properties"].get("pattern").is_some());
        assert_eq!(schema["properties"]["pattern"]["type"], "object");

        // 4. 验证内部的 pattern 校验字段被移除并转为描述
        assert!(schema["properties"]["pattern"]["properties"]["regex"]
            .get("pattern")
            .is_none());
        assert!(
            schema["properties"]["pattern"]["properties"]["regex"]["description"]
                .as_str()
                .unwrap()
                .contains("[Constraint: pattern: ^[a-z]+$]")
        );

        // 5. 验证联合类型被降级为单一类型 (Protobuf 兼容性)
        assert_eq!(schema["properties"]["unit"]["type"], "string");

        // 6. 验证元数据字段被移除
        assert!(schema.get("$schema").is_none());
    }

    #[test]
    fn test_type_fallback() {
        // Test ["string", "null"] -> "string"
        let mut s1 = json!({"type": ["string", "null"]});
        clean_json_schema(&mut s1);
        assert_eq!(s1["type"], "string");

        // Test ["integer", "null"] -> "integer" (and lowercase check if needed, though usually integer)
        let mut s2 = json!({"type": ["integer", "null"]});
        clean_json_schema(&mut s2);
        assert_eq!(s2["type"], "integer");
    }

    #[test]
    fn test_flatten_refs() {
        let mut schema = json!({
            "$defs": {
                "Address": {
                    "type": "object",
                    "properties": {
                        "city": { "type": "string" }
                    }
                }
            },
            "properties": {
                "home": { "$ref": "#/$defs/Address" }
            }
        });

        clean_json_schema(&mut schema);

        // 验证引用被展开且类型转为小写
        assert_eq!(schema["properties"]["home"]["type"], "object");
        assert_eq!(
            schema["properties"]["home"]["properties"]["city"]["type"],
            "string"
        );
    }

    #[test]
    fn test_clean_json_schema_missing_required() {
        let mut schema = json!({
            "type": "object",
            "properties": {
                "existing_prop": { "type": "string" }
            },
            "required": ["existing_prop", "missing_prop"]
        });

        clean_json_schema(&mut schema);

        // 验证 missing_prop 被从 required 中移除
        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0].as_str().unwrap(), "existing_prop");
    }

    // [NEW TEST] 验证 anyOf 类型提取
    #[test]
    fn test_anyof_type_extraction() {
        // 测试 FastMCP 风格的 Optional[str] schema
        let mut schema = json!({
            "type": "object",
            "properties": {
                "testo": {
                    "anyOf": [
                        {"type": "string"},
                        {"type": "null"}
                    ],
                    "default": null,
                    "title": "Testo"
                },
                "importo": {
                    "anyOf": [
                        {"type": "number"},
                        {"type": "null"}
                    ],
                    "default": null,
                    "title": "Importo"
                },
                "attivo": {
                    "type": "boolean",
                    "title": "Attivo"
                }
            }
        });

        clean_json_schema(&mut schema);

        // 验证 anyOf 被移除
        assert!(schema["properties"]["testo"].get("anyOf").is_none());
        assert!(schema["properties"]["importo"].get("anyOf").is_none());

        // 验证 type 被正确提取
        assert_eq!(schema["properties"]["testo"]["type"], "string");
        assert_eq!(schema["properties"]["importo"]["type"], "number");
        assert_eq!(schema["properties"]["attivo"]["type"], "boolean");

        // 验证 default 被移除 (白名单之外)
        assert!(schema["properties"]["testo"].get("default").is_none());
    }

    // [NEW TEST] 验证 oneOf 类型提取
    #[test]
    fn test_oneof_type_extraction() {
        let mut schema = json!({
            "properties": {
                "value": {
                    "oneOf": [
                        {"type": "integer"},
                        {"type": "null"}
                    ]
                }
            }
        });

        clean_json_schema(&mut schema);

        assert!(schema["properties"]["value"].get("oneOf").is_none());
        assert_eq!(schema["properties"]["value"]["type"], "integer");
    }

    // [NEW TEST] 验证已有 type 不被覆盖
    #[test]
    fn test_existing_type_preserved() {
        let mut schema = json!({
            "properties": {
                "name": {
                    "type": "string",
                    "anyOf": [
                        {"type": "number"}
                    ]
                }
            }
        });

        clean_json_schema(&mut schema);

        // type 已存在，不应被 anyOf 中的类型覆盖
        assert_eq!(schema["properties"]["name"]["type"], "string");
        assert!(schema["properties"]["name"].get("anyOf").is_none());
    }

    // [NEW TEST] 验证 Issue #815: anyOf 内部属性不丢失
    #[test]
    fn test_issue_815_anyof_properties_preserved() {
        let mut schema = json!({
            "type": "object",
            "properties": {
                "config": {
                    "anyOf": [
                        {
                            "type": "object",
                            "properties": {
                                "path": { "type": "string" },
                                "recursive": { "type": "boolean" }
                            },
                            "required": ["path"]
                        },
                        { "type": "null" }
                    ]
                }
            }
        });

        clean_json_schema(&mut schema);

        let config = &schema["properties"]["config"];
        
        // 1. 验证类型被提取
        assert_eq!(config["type"], "object");
        
        // 2. 验证 anyOf 内部的 properties 被合并上来了
        assert!(config.get("properties").is_some());
        assert_eq!(config["properties"]["path"]["type"], "string");
        assert_eq!(config["properties"]["recursive"]["type"], "boolean");
        
        // 3. 验证 required 被合并上来了
        let req = config["required"].as_array().unwrap();
        assert!(req.iter().any(|v| v == "path"));
        
        // 4. 验证 anyOf 字段本身被移除
        assert!(config.get("anyOf").is_none());
        
        // 5. 验证没有因为“空”而注入 reason (因为我们保留了属性)
        assert!(config["properties"].get("reason").is_none());
    }

    // [NEW TEST] 验证安全检查：不应处理非 Schema 对象（保护工具调用）
    #[test]
    fn test_clean_json_schema_on_non_schema_object() {
        // 模拟 request.rs 中转换了一半的 functionCall 对象
        let mut tool_call = json!({
            "functionCall": {
                "name": "local_shell_call",
                "args": { "command": ["ls"] },
                "id": "call_123"
            }
        });

        // 调用清洗逻辑
        clean_json_schema(&mut tool_call);

        // 验证：这些非 Schema 字段不应被移除（因为不符合 looks_like_schema 判定）
        let fc = &tool_call["functionCall"];
        assert_eq!(fc["name"], "local_shell_call");
        assert_eq!(fc["args"]["command"][0], "ls");
        assert_eq!(fc["id"], "call_123");
    }

    // [NEW TEST] 验证 Nullable 处理
    #[test]
    fn test_nullable_handling_with_description() {
        let mut schema = json!({
            "type": ["string", "null"],
            "description": "User name"
        });

        clean_json_schema(&mut schema);

        // 验证 type 被降级，且描述被追加 (nullable)
        assert_eq!(schema["type"], "string");
        assert!(schema["description"].as_str().unwrap().contains("User name"));
        assert!(schema["description"].as_str().unwrap().contains("(nullable)"));
    }
}
