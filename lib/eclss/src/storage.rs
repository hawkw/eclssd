use serde::{de::DeserializeOwned, Serialize};

#[allow(async_fn_in_trait)]
pub trait Store {
    type Error;
    async fn load<T: DeserializeOwned>(&mut self) -> Result<Option<T>, Self::Error>;
    async fn store<T: Serialize>(&mut self, value: &T) -> Result<(), Self::Error>;
}

impl Store for () {
    type Error = core::convert::Infallible;

    async fn load<T: DeserializeOwned>(&mut self) -> Result<Option<T>, Self::Error> {
        Ok(None)
    }

    async fn store<T: Serialize>(&mut self, _: &T) -> Result<(), Self::Error> {
        Ok(())
    }
}
