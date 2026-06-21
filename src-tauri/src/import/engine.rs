use std::path::Path;

use boa_engine::{
    Context, JsArgs, JsError, JsValue, Source, js_string, native_function::NativeFunction,
    object::FunctionObjectBuilder, property::PropertyDescriptorBuilder,
};
use serde::{Deserialize, Serialize};

use super::extract::ExtractedContent;
use crate::AppError;

// ── Output types from JS transform ──────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ParsedTransaction {
    pub id: String,
    pub booking_date: String,
    pub value_date: String,
    pub reference: String,
    pub text: String,
    pub amount: i64,
    pub balance: i64,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ParseResult {
    Checking {
        account_id: String,
        transactions: Vec<ParsedTransaction>,
    },
    Savings {
        account_id: String,
        transactions: Vec<ParsedTransaction>,
    },
    Investment {
        account_id: String,
        month: String,
        balance: i64,
    },
}

// ── Unit metadata (no transform fn — extracted during discovery) ─────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitMeta {
    pub key: String,
    pub name: String,
    pub bank: String,
    pub format: String,
    pub account_type: String,
    pub account_source: String,
    pub currency: String,
}

// ── Run the transform function from a script file ────────────────────────────

// ── Run the transform function from a script file ────────────────────────────

pub fn run_transform(
    script_path: &Path,
    unit_key: &str,
    content: ExtractedContent,
) -> Result<(ParseResult, Vec<String>), AppError> {
    let source_text = std::fs::read_to_string(script_path)
        .map_err(|e| AppError::Io(format!("read script {}: {e}", script_path.display())))?;

    SCRIPT_LOGS.with(|logs| logs.borrow_mut().clear());

    let mut context = build_context();

    let result_json = eval_transform(&mut context, &source_text, unit_key, content)
        .map_err(|e| AppError::Internal(format!("JS engine error: {e}")))?;

    let parsed = serde_json::from_str::<ParseResult>(&result_json).map_err(|e| {
        AppError::Internal(format!("JS output parse error: {e}\nJSON: {result_json}"))
    })?;

    let logs = SCRIPT_LOGS.with(|l| l.borrow().clone());
    Ok((parsed, logs))
}

// ── Run a test transform directly from memory (custom code string) ───────────

pub fn run_test_transform(
    script_code: &str,
    unit_key: &str,
    content: ExtractedContent,
) -> Result<(String, Vec<String>), AppError> {
    SCRIPT_LOGS.with(|logs| logs.borrow_mut().clear());

    let mut context = build_context();
    let result_json = eval_transform(&mut context, script_code, unit_key, content)
        .map_err(|e| AppError::Internal(format!("JS engine error: {e}")))?;

    let logs = SCRIPT_LOGS.with(|l| l.borrow().clone());
    Ok((result_json, logs))
}

// ── Extract unit metadata from a script (no transform call) ─────────────────

pub fn extract_units(script_path: &Path) -> Result<Vec<UnitMeta>, AppError> {
    let source_text = std::fs::read_to_string(script_path)
        .map_err(|e| AppError::Io(format!("read script {}: {e}", script_path.display())))?;
    extract_units_source(&source_text)
}

pub fn extract_units_source(source_text: &str) -> Result<Vec<UnitMeta>, AppError> {
    let mut context = build_context();

    let units_json = eval_units(&mut context, source_text)
        .map_err(|e| AppError::Internal(format!("JS metadata error: {e}")))?;

    serde_json::from_str::<Vec<UnitMeta>>(&units_json)
        .map_err(|e| AppError::Internal(format!("JS units parse error: {e}")))
}

use std::cell::RefCell;

thread_local! {
    static SCRIPT_LOGS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn js_console_log(
    _this: &JsValue,
    args: &[JsValue],
    ctx: &mut Context,
) -> Result<JsValue, JsError> {
    let mut s = String::new();
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        let val_str = arg
            .to_string(ctx)
            .map(|js_str| js_str.to_std_string_lossy().to_string())
            .unwrap_or_else(|_| "[error converting value to string]".to_string());
        s.push_str(&val_str);
    }
    SCRIPT_LOGS.with(|logs| {
        logs.borrow_mut().push(s);
    });
    Ok(JsValue::undefined())
}

fn js_md5(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> Result<JsValue, JsError> {
    let input = args
        .get_or_undefined(0)
        .to_string(ctx)
        .map(|s| s.to_std_string_lossy())
        .unwrap_or_default();

    use md5::{Digest, Md5};
    let hash = Md5::digest(input.as_bytes());
    let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
    Ok(JsValue::from(js_string!(hex.as_str())))
}

// ── Build a Boa Context with our globals ─────────────────────────────────────

fn build_context() -> Context {
    let mut ctx = Context::default();

    let md5_fn = FunctionObjectBuilder::new(ctx.realm(), NativeFunction::from_fn_ptr(js_md5))
        .name(js_string!("md5"))
        .length(1)
        .build();

    ctx.global_object()
        .define_property_or_throw(
            js_string!("md5"),
            PropertyDescriptorBuilder::new()
                .value(md5_fn)
                .writable(true)
                .enumerable(false)
                .configurable(true)
                .build(),
            &mut ctx,
        )
        .expect("md5 global registration failed");

    // Register global console object and log method
    let console = ctx
        .global_object()
        .get(js_string!("console"), &mut ctx)
        .unwrap_or(JsValue::undefined());
    let console_obj = if console.is_undefined() {
        let obj = boa_engine::object::JsObject::default(ctx.intrinsics());
        ctx.global_object()
            .set(js_string!("console"), obj.clone(), true, &mut ctx)
            .unwrap();
        obj
    } else {
        console.as_object().unwrap().clone()
    };

    let log_fn =
        FunctionObjectBuilder::new(ctx.realm(), NativeFunction::from_fn_ptr(js_console_log))
            .name(js_string!("log"))
            .length(1)
            .build();

    console_obj
        .define_property_or_throw(
            js_string!("log"),
            PropertyDescriptorBuilder::new()
                .value(log_fn)
                .writable(true)
                .enumerable(false)
                .configurable(true)
                .build(),
            &mut ctx,
        )
        .unwrap();

    ctx
}

// ── Evaluate the script, call transform(data), return JSON string ─────────────

fn eval_transform(
    context: &mut Context,
    source: &str,
    unit_key: &str,
    content: ExtractedContent,
) -> Result<String, String> {
    let wrapped = wrap_module(source);

    context
        .eval(Source::from_bytes(wrapped.as_bytes()))
        .map_err(|e| e.to_string())?;

    let data_json = match content {
        ExtractedContent::Rows(rows) => serde_json::to_string(&rows).unwrap_or("[]".into()),
        ExtractedContent::Text(text) => {
            let obj = serde_json::json!({ "text": text });
            obj.to_string()
        }
    };

    let call = format!(
        r#"(function() {{
            var unit = __defaultExport.units.find(function(u) {{ return u.key === "{unit_key}"; }});
            if (!unit) throw new Error("unit not found: {unit_key}");
            var rawData = {data_json};
            var data = Array.isArray(rawData) ? {{ rows: rawData }} : rawData;
            var result = unit.transform(data);
            return JSON.stringify(result);
        }})()"#
    );

    let result = context
        .eval(Source::from_bytes(call.as_bytes()))
        .map_err(|e| e.to_string())?;

    result
        .as_string()
        .map(|s| s.to_std_string_lossy())
        .ok_or_else(|| "transform() did not return a string".into())
}

fn eval_units(context: &mut Context, source: &str) -> Result<String, String> {
    let wrapped = wrap_module(source);

    context
        .eval(Source::from_bytes(wrapped.as_bytes()))
        .map_err(|e| e.to_string())?;

    let call = r#"(function() {
        var units = (__defaultExport.units || []).map(function(u) {
            return {
                key:            u.key,
                name:           u.name,
                bank:           __defaultExport.bank,
                format:         u.format,
                account_type:   u.account_type,
                account_source: u.account_source,
                currency:       u.currency
            };
        });
        return JSON.stringify(units);
    })()"#;

    let result = context
        .eval(Source::from_bytes(call.as_bytes()))
        .map_err(|e| e.to_string())?;

    result
        .as_string()
        .map(|s| s.to_std_string_lossy())
        .ok_or_else(|| "units extraction did not return a string".into())
}

// Strip `export default` so Boa can run the file as a plain script and assign
// the default export to a known variable name.
fn wrap_module(source: &str) -> String {
    source.replacen("export default", "var __defaultExport =", 1)
}
