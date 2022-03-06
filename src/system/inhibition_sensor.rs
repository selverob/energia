use crate::armaf::{Actor};
use anyhow::Result;
use async_trait::async_trait;
use logind_zbus::manager::{self};
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub struct GetInhibitions;

pub struct InhibitionSensor {
    connection: zbus::Connection,
    manager_proxy: Option<logind_zbus::manager::ManagerProxy<'static>>,
}

impl InhibitionSensor {
    pub fn new(connection: zbus::Connection) -> InhibitionSensor {
        InhibitionSensor {
            connection,
            manager_proxy: None,
        }
    }
}

#[async_trait]
impl Actor<GetInhibitions, Vec<manager::Inhibitor>> for InhibitionSensor {
    fn get_name(&self) -> String {
        "InhibitionSensor".to_owned()
    }

    async fn handle_message(&mut self, _: GetInhibitions) -> Result<Vec<manager::Inhibitor>> {
        Ok(self
            .manager_proxy
            .as_ref()
            .unwrap()
            .list_inhibitors()
            .await?)
    }

    async fn initialize(&mut self) -> Result<()> {
        self.manager_proxy = Some(logind_zbus::manager::ManagerProxy::new(&self.connection).await?);
        Ok(())
    }
}
