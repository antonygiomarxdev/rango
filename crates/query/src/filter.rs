use bson::{Bson, Document};
use rango_types::RangoError;
use tracing::instrument;

/// Evaluate a MongoDB-style filter against a document.
#[instrument(skip(doc, filter))]
pub fn matches(doc: &Document, filter: &Document) -> Result<bool, RangoError> {
    for (key, value) in filter {
        if !match_operator(doc, key, value)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn match_operator(doc: &Document, key: &str, value: &Bson) -> Result<bool, RangoError> {
    match key {
        "$and" => match_array(doc, value, |d, f| matches(d, f)),
        "$or" => match_array_or(doc, value),
        _ => {
            // Field operator or simple equality
            if key.starts_with('$') {
                return Err(RangoError::InvalidQueryOperator(key.to_string()));
            }
            match_field(doc, key, value)
        }
    }
}

fn match_array(
    doc: &Document,
    value: &Bson,
    matcher: fn(&Document, &Document) -> Result<bool, RangoError>,
) -> Result<bool, RangoError> {
    match value.as_array() {
        Some(arr) => {
            for item in arr {
                match item.as_document() {
                    Some(filter_doc) => {
                        if !matcher(doc, filter_doc)? {
                            return Ok(false);
                        }
                    }
                    None => return Err(RangoError::InvalidQueryOperator("$and/$or expects array of documents".to_string())),
                }
            }
            Ok(true)
        }
        None => Err(RangoError::InvalidQueryOperator("$and/$or expects array".to_string())),
    }
}

fn match_array_or(doc: &Document, value: &Bson) -> Result<bool, RangoError> {
    match value.as_array() {
        Some(arr) => {
            if arr.is_empty() {
                return Ok(true);
            }
            for item in arr {
                match item.as_document() {
                    Some(filter_doc) => {
                        if matches(doc, filter_doc)? {
                            return Ok(true);
                        }
                    }
                    None => return Err(RangoError::InvalidQueryOperator("$or expects array of documents".to_string())),
                }
            }
            Ok(false)
        }
        None => Err(RangoError::InvalidQueryOperator("$or expects array".to_string())),
    }
}

fn match_field(doc: &Document, field: &str, value: &Bson) -> Result<bool, RangoError> {
    match value {
        Bson::Document(op_doc) => {
            // Operator document: { $gt: 5, $lt: 10 }
            for (op, op_value) in op_doc {
                match op.as_str() {
                    "$eq" => {
                        if !compare_eq(doc.get(field), Some(op_value)) {
                            return Ok(false);
                        }
                    }
                    "$gt" => {
                        if !compare_gt(doc.get(field), Some(op_value))? {
                            return Ok(false);
                        }
                    }
                    "$gte" => {
                        if !compare_gte(doc.get(field), Some(op_value))? {
                            return Ok(false);
                        }
                    }
                    "$lt" => {
                        if !compare_lt(doc.get(field), Some(op_value))? {
                            return Ok(false);
                        }
                    }
                    "$lte" => {
                        if !compare_lte(doc.get(field), Some(op_value))? {
                            return Ok(false);
                        }
                    }
                    "$in" => {
                        if !compare_in(doc.get(field), op_value)? {
                            return Ok(false);
                        }
                    }
                    _ => return Err(RangoError::InvalidQueryOperator(op.clone())),
                }
            }
            Ok(true)
        }
        _ => {
            // Implicit $eq
            Ok(compare_eq(doc.get(field), Some(value)))
        }
    }
}

fn compare_eq(a: Option<&Bson>, b: Option<&Bson>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => {
            // Try strict equality first
            if a == b {
                return true;
            }
            // Cross-type numeric equality
            match (a, b) {
                (Bson::Int32(a), Bson::Int64(b)) => *a as i64 == *b,
                (Bson::Int64(a), Bson::Int32(b)) => *a == *b as i64,
                (Bson::Int32(a), Bson::Double(b)) => *a as f64 == *b,
                (Bson::Double(a), Bson::Int32(b)) => *a == *b as f64,
                (Bson::Int64(a), Bson::Double(b)) => *a as f64 == *b,
                (Bson::Double(a), Bson::Int64(b)) => *a == *b as f64,
                _ => false,
            }
        }
        (None, None) => true,
        _ => false,
    }
}

fn compare_gt(a: Option<&Bson>, b: Option<&Bson>) -> Result<bool, RangoError> {
    match (a, b) {
        (Some(a), Some(b)) => compare_values(a, b).map(|ord| ord == std::cmp::Ordering::Greater),
        _ => Ok(false),
    }
}

fn compare_gte(a: Option<&Bson>, b: Option<&Bson>) -> Result<bool, RangoError> {
    match (a, b) {
        (Some(a), Some(b)) => compare_values(a, b).map(|ord| ord != std::cmp::Ordering::Less),
        _ => Ok(false),
    }
}

fn compare_lt(a: Option<&Bson>, b: Option<&Bson>) -> Result<bool, RangoError> {
    match (a, b) {
        (Some(a), Some(b)) => compare_values(a, b).map(|ord| ord == std::cmp::Ordering::Less),
        _ => Ok(false),
    }
}

fn compare_lte(a: Option<&Bson>, b: Option<&Bson>) -> Result<bool, RangoError> {
    match (a, b) {
        (Some(a), Some(b)) => compare_values(a, b).map(|ord| ord != std::cmp::Ordering::Greater),
        _ => Ok(false),
    }
}

fn compare_in(field_value: Option<&Bson>, array: &Bson) -> Result<bool, RangoError> {
    match array.as_array() {
        Some(arr) => {
            for item in arr {
                if compare_eq(field_value, Some(item)) {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        None => Err(RangoError::InvalidQueryOperator("$in expects array".to_string())),
    }
}

fn compare_values(a: &Bson, b: &Bson) -> Result<std::cmp::Ordering, RangoError> {
    match (a, b) {
        (Bson::Int32(a), Bson::Int32(b)) => Ok(a.cmp(b)),
        (Bson::Int64(a), Bson::Int64(b)) => Ok(a.cmp(b)),
        (Bson::Double(a), Bson::Double(b)) => {
            a.partial_cmp(b)
                .ok_or_else(|| RangoError::InvalidQueryOperator("Cannot compare NaN values".to_string()))
        }
        (Bson::String(a), Bson::String(b)) => Ok(a.cmp(b)),
        (Bson::DateTime(a), Bson::DateTime(b)) => Ok(a.cmp(b)),
        // Cross-type numeric comparisons
        (Bson::Int32(a), Bson::Int64(b)) => Ok((*a as i64).cmp(b)),
        (Bson::Int64(a), Bson::Int32(b)) => Ok(a.cmp(&(*b as i64))),
        (Bson::Int32(a), Bson::Double(b)) => {
            (*a as f64).partial_cmp(b)
                .ok_or_else(|| RangoError::InvalidQueryOperator("Cannot compare NaN values".to_string()))
        }
        (Bson::Double(a), Bson::Int32(b)) => {
            a.partial_cmp(&(*b as f64))
                .ok_or_else(|| RangoError::InvalidQueryOperator("Cannot compare NaN values".to_string()))
        }
        (Bson::Int64(a), Bson::Double(b)) => {
            (*a as f64).partial_cmp(b)
                .ok_or_else(|| RangoError::InvalidQueryOperator("Cannot compare NaN values".to_string()))
        }
        (Bson::Double(a), Bson::Int64(b)) => {
            a.partial_cmp(&(*b as f64))
                .ok_or_else(|| RangoError::InvalidQueryOperator("Cannot compare NaN values".to_string()))
        }
        _ => Err(RangoError::InvalidQueryOperator(
            format!("Cannot compare {:?} with {:?}", a, b)
        )),
    }
}
