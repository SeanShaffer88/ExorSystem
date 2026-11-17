//! JSONLogic evaluator (subset sufficient for cleanup policies).
//!
//! Operators: `var`, `if`, `and`, `or`, `!`, `!!`, `==`, `===`, `!=`, `!==`, `>`, `>=`, `<`, `<=`
//! (with the 3-argument "between" form), `in`, `cat`, `+`, `-`, `*`, `/`, `%`, `min`, `max`.
//! ExorSystem extensions (case-insensitive, for Windows paths): `starts_with`, `ends_with`.
//! Unknown operators are errors, never silently false.

use crate::error::{EngineError, Result};
use serde_json::{Number, Value};

const MAX_DEPTH: usize = 64;

fn err<T>(msg: impl Into<String>) -> Result<T> {
    Err(EngineError::Policy(msg.into()))
}

/// JSONLogic truthiness.
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(_) => true,
    }
}

fn to_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        Value::Null => Some(0.0),
        _ => None,
    }
}

fn num(f: f64) -> Value {
    if f.fract() == 0.0 && f.abs() < 1e15 {
        Value::Number(Number::from(f as i64))
    } else {
        Number::from_f64(f).map_or(Value::Null, Value::Number)
    }
}

fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

fn loose_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Null, _) | (_, Value::Null) => false,
        (Value::String(x), Value::String(y)) => x == y,
        (Value::Array(_) | Value::Object(_), _) | (_, Value::Array(_) | Value::Object(_)) => a == b,
        _ => matches!((to_f64(a), to_f64(b)), (Some(x), Some(y)) if x == y),
    }
}

fn strict_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        _ => a == b,
    }
}

fn get_var(data: &Value, path: &Value) -> Value {
    let key = match path {
        Value::Null => return data.clone(),
        Value::String(s) if s.is_empty() => return data.clone(),
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => return Value::Null,
    };
    let mut cur = data;
    for part in key.split('.') {
        cur = match cur {
            Value::Object(m) => match m.get(part) {
                Some(v) => v,
                None => return Value::Null,
            },
            Value::Array(a) => match part.parse::<usize>().ok().and_then(|i| a.get(i)) {
                Some(v) => v,
                None => return Value::Null,
            },
            _ => return Value::Null,
        };
    }
    cur.clone()
}

fn arg_list(args: &Value) -> Vec<Value> {
    match args {
        Value::Array(a) => a.clone(),
        other => vec![other.clone()],
    }
}

fn numbers(op: &str, vals: &[Value]) -> Result<Vec<f64>> {
    vals.iter()
        .map(|v| {
            to_f64(v).ok_or_else(|| {
                EngineError::Policy(format!("`{op}` needs numeric operands, got {v}"))
            })
        })
        .collect()
}

fn cmp(op: &str, vals: &[Value]) -> Result<Value> {
    let n = |i: usize| vals.get(i).and_then(to_f64);
    let test = |a: f64, b: f64| match op {
        ">" => a > b,
        ">=" => a >= b,
        "<" => a < b,
        _ => a <= b,
    };
    match vals.len() {
        2 => Ok(Value::Bool(
            matches!((n(0), n(1)), (Some(a), Some(b)) if test(a, b)),
        )),
        3 => Ok(Value::Bool(match (n(0), n(1), n(2)) {
            (Some(a), Some(b), Some(c)) => test(a, b) && test(b, c),
            _ => false,
        })),
        k => err(format!("`{op}` takes 2 or 3 arguments, got {k}")),
    }
}

fn two<'a>(op: &str, vals: &'a [Value]) -> Result<(&'a Value, &'a Value)> {
    match vals {
        [a, b] => Ok((a, b)),
        _ => err(format!(
            "`{op}` takes exactly 2 arguments, got {}",
            vals.len()
        )),
    }
}

fn eval(logic: &Value, data: &Value, depth: usize) -> Result<Value> {
    if depth > MAX_DEPTH {
        return err("expression nesting too deep");
    }
    match logic {
        Value::Array(a) => Ok(Value::Array(
            a.iter()
                .map(|x| eval(x, data, depth + 1))
                .collect::<Result<_>>()?,
        )),
        Value::Object(m) => {
            if m.len() != 1 {
                return err("an operator object must have exactly one key");
            }
            let Some((op, raw)) = m.iter().next() else {
                return err("empty operator object");
            };
            let raw = arg_list(raw);
            match op.as_str() {
                "if" => {
                    let mut i = 0;
                    while i + 1 < raw.len() {
                        if truthy(&eval(&raw[i], data, depth + 1)?) {
                            return eval(&raw[i + 1], data, depth + 1);
                        }
                        i += 2;
                    }
                    if i < raw.len() {
                        eval(&raw[i], data, depth + 1)
                    } else {
                        Ok(Value::Null)
                    }
                }
                "and" => {
                    let mut last = Value::Bool(true);
                    for r in &raw {
                        last = eval(r, data, depth + 1)?;
                        if !truthy(&last) {
                            break;
                        }
                    }
                    Ok(last)
                }
                "or" => {
                    let mut last = Value::Bool(false);
                    for r in &raw {
                        last = eval(r, data, depth + 1)?;
                        if truthy(&last) {
                            break;
                        }
                    }
                    Ok(last)
                }
                other => {
                    let vals: Vec<Value> = raw
                        .iter()
                        .map(|r| eval(r, data, depth + 1))
                        .collect::<Result<_>>()?;
                    eval_op(other, &vals, data)
                }
            }
        }
        other => Ok(other.clone()),
    }
}

fn eval_op(op: &str, vals: &[Value], data: &Value) -> Result<Value> {
    match op {
        "var" => {
            let v = get_var(data, vals.first().unwrap_or(&Value::Null));
            Ok(if v.is_null() && vals.len() > 1 {
                vals[1].clone()
            } else {
                v
            })
        }
        "!" => Ok(Value::Bool(!vals.first().is_some_and(truthy))),
        "!!" => Ok(Value::Bool(vals.first().is_some_and(truthy))),
        "==" => two(op, vals).map(|(a, b)| Value::Bool(loose_eq(a, b))),
        "!=" => two(op, vals).map(|(a, b)| Value::Bool(!loose_eq(a, b))),
        "===" => two(op, vals).map(|(a, b)| Value::Bool(strict_eq(a, b))),
        "!==" => two(op, vals).map(|(a, b)| Value::Bool(!strict_eq(a, b))),
        ">" | ">=" | "<" | "<=" => cmp(op, vals),
        "in" => {
            let (needle, hay) = two(op, vals)?;
            Ok(Value::Bool(match hay {
                Value::String(h) => h.contains(&text(needle)),
                Value::Array(a) => a.iter().any(|x| loose_eq(x, needle)),
                _ => false,
            }))
        }
        "starts_with" | "ends_with" => {
            let (a, b) = two(op, vals)?;
            let (a, b) = (text(a).to_lowercase(), text(b).to_lowercase());
            Ok(Value::Bool(if op == "starts_with" {
                a.starts_with(&b)
            } else {
                a.ends_with(&b)
            }))
        }
        "cat" => Ok(Value::String(vals.iter().map(text).collect())),
        "+" => Ok(num(numbers(op, vals)?.iter().sum())),
        "*" => Ok(num(numbers(op, vals)?.iter().product())),
        "-" => match numbers(op, vals)?.as_slice() {
            [a] => Ok(num(-a)),
            [a, b] => Ok(num(a - b)),
            _ => err("`-` takes 1 or 2 arguments"),
        },
        "/" | "%" => match numbers(op, vals)?.as_slice() {
            [_, b] if *b == 0.0 => err("division by zero"),
            [a, b] => Ok(num(if op == "/" { a / b } else { a % b })),
            _ => err(format!("`{op}` takes exactly 2 arguments")),
        },
        "min" | "max" => {
            let n = numbers(op, vals)?;
            let pick = if op == "min" {
                n.iter().copied().reduce(f64::min)
            } else {
                n.iter().copied().reduce(f64::max)
            };
            Ok(pick.map_or(Value::Null, num))
        }
        other => err(format!("unknown operator `{other}`")),
    }
}

/// Evaluate `logic` against `data`.
pub fn apply(logic: &Value, data: &Value) -> Result<Value> {
    eval(logic, data, 0)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

    fn t(logic: Value, data: Value) -> Value {
        apply(&logic, &data).unwrap()
    }

    #[test]
    fn var_paths_and_defaults() {
        let d = json!({"a": {"b": [10, 20]}, "s": "x"});
        assert_eq!(t(json!({"var": "a.b.1"}), d.clone()), json!(20));
        assert_eq!(t(json!({"var": ["nope", 5]}), d.clone()), json!(5));
        assert_eq!(t(json!({"var": ""}), d.clone()), d);
    }

    #[test]
    fn comparisons_and_between() {
        assert_eq!(
            t(json!({">=": [{"var": "c"}, 0.7]}), json!({"c": 0.7})),
            json!(true)
        );
        assert_eq!(t(json!({"<": [1, 2, 3]}), json!({})), json!(true));
        assert_eq!(t(json!({"<": [1, 5, 3]}), json!({})), json!(false));
        assert_eq!(t(json!({"==": [1, "1"]}), json!({})), json!(true));
        assert_eq!(t(json!({"===": [1, "1"]}), json!({})), json!(false));
        assert_eq!(t(json!({"!=": [1, 2]}), json!({})), json!(true));
    }

    #[test]
    fn logic_short_circuits_and_if() {
        assert_eq!(
            t(json!({"or": [false, 0, "hit", {"/": [1, 0]}]}), json!({})),
            json!("hit")
        );
        assert_eq!(
            t(json!({"and": [true, 0, {"/": [1, 0]}]}), json!({})),
            json!(0)
        );
        assert_eq!(
            t(json!({"if": [false, "a", true, "b", "c"]}), json!({})),
            json!("b")
        );
        assert_eq!(t(json!({"if": [false, "a", "c"]}), json!({})), json!("c"));
        assert_eq!(t(json!({"!": [[]]}), json!({})), json!(true));
        assert_eq!(t(json!({"!!": ["x"]}), json!({})), json!(true));
    }

    #[test]
    fn in_cat_and_arithmetic() {
        assert_eq!(t(json!({"in": ["Foo", "Foobar"]}), json!({})), json!(true));
        assert_eq!(t(json!({"in": [2, [1, 2]]}), json!({})), json!(true));
        assert_eq!(
            t(json!({"cat": ["a", 1, null, "b"]}), json!({})),
            json!("a1b")
        );
        assert_eq!(t(json!({"+": [1, 2, "3"]}), json!({})), json!(6));
        assert_eq!(t(json!({"%": [7, 4]}), json!({})), json!(3));
        assert_eq!(t(json!({"max": [1, 9, 3]}), json!({})), json!(9));
        assert_eq!(t(json!({"-": [5]}), json!({})), json!(-5));
    }

    #[test]
    fn path_extensions_are_case_insensitive() {
        let d = json!({"path": "C:\\Windows\\System32\\x.DLL"});
        assert_eq!(
            t(
                json!({"starts_with": [{"var": "path"}, "c:\\windows\\"]}),
                d.clone()
            ),
            json!(true)
        );
        assert_eq!(
            t(json!({"ends_with": [{"var": "path"}, ".dll"]}), d),
            json!(true)
        );
    }

    #[test]
    fn errors_are_explicit() {
        assert!(apply(&json!({"bogus": [1]}), &json!({})).is_err());
        assert!(apply(&json!({"/": [1, 0]}), &json!({})).is_err());
        assert!(apply(&json!({"+": ["a", 1]}), &json!({})).is_err());
        assert!(apply(&json!({"a": 1, "b": 2}), &json!({})).is_err());
        let mut deep = json!(1);
        for _ in 0..100 {
            deep = json!({"!": [deep]});
        }
        assert!(apply(&deep, &json!({})).is_err());
    }
}
