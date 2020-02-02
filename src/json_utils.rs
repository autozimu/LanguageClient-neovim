use failure::Fail;
use serde::de::DeserializeOwned;
use serde_json::Value;

pub(crate) fn from_value<T>(value: &Value) -> serde_json::Result<T>
where
    T: DeserializeOwned,
{
    T::deserialize(value)
}

pub(crate) fn from_value_opt<T>(value: &Value) -> serde_json::Result<Option<T>>
where
    T: DeserializeOwned,
{
    match value {
        Value::Null => Ok(None),
        _ => T::deserialize(value).map(|value| Some(value)),
    }
}

pub(crate) fn from_opt_value<T>(value: Option<&Value>) -> Result<T, Error>
where
    T: DeserializeOwned,
{
    value.map_or(Err(Error::InvalidType), |value| {
        from_value(value).map_err(Error::SerdeJson)
    })
}

pub(crate) fn as_value_vec(value: &Value) -> Result<&Vec<Value>, Error> {
    value.as_array().ok_or(Error::InvalidArray)
}

pub(crate) fn as_value_opt_vec(value: &Value) -> Result<Option<&Vec<Value>>, Error> {
    match value {
        Value::Null => Ok(None),
        Value::Array(vec) => Ok(Some(vec)),
        _ => Err(Error::InvalidArray),
    }
}

pub(crate) fn iter_vec<'v, T>(
    value: &'v Value,
) -> Result<impl Iterator<Item = Result<T, Error>> + 'v, Error>
where
    T: DeserializeOwned,
{
    let iter = as_value_vec(value)?
        .iter()
        .map(|value| T::deserialize(value).map_err(Error::SerdeJson));
    Ok(iter)
}

pub(crate) fn into_iter_vec<T>(
    value: Value,
) -> Result<impl Iterator<Item = Result<T, Error>>, Error>
where
    T: DeserializeOwned,
{
    let vec = match value {
        Value::Array(vec) => vec,
        _ => return Err(Error::InvalidArray),
    };
    let iter = vec
        .into_iter()
        .map(|value| T::deserialize(value).map_err(Error::SerdeJson));
    Ok(iter)
}

pub(crate) fn iter_opt_vec<'v, T>(
    value: &'v Value,
) -> Result<Option<impl Iterator<Item = Result<T, Error>> + 'v>, Error>
where
    T: DeserializeOwned,
{
    let iter = as_value_opt_vec(value)?.map(|vec| {
        vec.iter()
            .map(|value| T::deserialize(value).map_err(Error::SerdeJson))
    });
    Ok(iter)
}

pub(crate) fn into_iter_map<K, V>(
    value: Value,
) -> Result<impl Iterator<Item = Result<(K, V), Error>>, Error>
where
    K: DeserializeOwned,
    V: DeserializeOwned,
{
    match value {
        Value::Object(map) => {
            let iter = map.into_iter().map(|(key, value)| {
                let key = serde_json::from_str(key.as_str()).map_err(Error::SerdeJson)?;
                let value = serde_json::from_value(value).map_err(Error::SerdeJson)?;
                Ok((key, value))
            });
            Ok(iter)
        }
        _ => Err(Error::InvalidObject),
    }
}

#[derive(Debug, Fail)]
#[non_exhaustive]
pub(crate) enum Error {
    #[fail(display = "unable to use a non-array as array type")]
    InvalidArray,
    #[fail(display = "unable to use a non-object as object type")]
    InvalidObject,
    #[fail(display = "unable to convert unexpected type")]
    InvalidType,
    #[fail(display = "serde_json error")]
    SerdeJson(#[fail(cause)] serde_json::Error),
}
