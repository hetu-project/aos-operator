use chrono::{Local, NaiveDateTime};
use db_sql::pg::entities::{
    clock_infos, job_result_param,
    prelude::{ClockInfos, JobResultParam},
};
use node_api::config::OperatorConfig;
use sea_orm::*;
use std::{sync::Arc, time::Duration};
use tracing::{error, info};

#[derive(Default, Clone)]
pub struct Storage {
    pub pg_db: Arc<DatabaseConnection>,
}

impl Storage {
    pub async fn new(config: Arc<OperatorConfig>) -> Self {
        // connect to pg db
        let url = format!("{}/{}", config.db.pg_db_url, config.db.pg_db_name);
        let mut opt = ConnectOptions::new(&url);
        opt.max_connections(config.db.max_connect_pool)
            .min_connections(config.db.min_connect_pool)
            .connect_timeout(Duration::from_secs(config.db.connect_timeout))
            .acquire_timeout(Duration::from_secs(config.db.acquire_timeout));

        let pg_db = Database::connect(opt.clone())
            .await
            .expect("failed to connect to database");
        info!(
            "max_connections={:?},connect timeout={:?}, acquire timeout={:?}",
            opt.get_max_connections().unwrap(),
            opt.get_connect_timeout().unwrap(),
            opt.get_acquire_timeout().unwrap()
        );
        let pg_db_arc = Arc::new(pg_db);
        Self {
            // operator_db,
            pg_db: pg_db_arc,
        }
    }

    // postgre inner api
    pub async fn sinker_clock(&self, message_id: String, raw_message: Vec<u8>) {
        let clock_info = clock_infos::ActiveModel {
            message_id: ActiveValue::Set(message_id),
            raw_message: ActiveValue::Set(raw_message),
            ..Default::default()
        };
        let res = ClockInfos::insert(clock_info)
            .exec(self.pg_db.as_ref())
            .await;
        if let Err(err) = res {
            error!("Insert clock_info error, err: {}", err);
        }
    }

    // postgre inner api
    pub async fn sinker_job(
        &self,
        job_id: String,
        username: String,
        result: String,
        tag: String,
        clock: String,
    ) {
        let job_result = job_result_param::ActiveModel {
            username: ActiveValue::Set(username),
            job_id: ActiveValue::Set(job_id),
            result: ActiveValue::Set(result),
            tag: ActiveValue::Set(tag),
            clock: ActiveValue::Set(clock),
            operator: ActiveValue::Set("".to_owned()),
            signature: ActiveValue::Set("".to_owned()),
            ..Default::default()
        };
        let res = JobResultParam::insert(job_result)
            .exec(self.pg_db.as_ref())
            .await;
        if let Err(err) = res {
            error!("Insert job_result_param error, err: {}", err);
        }
    }

    pub async fn get_clocks_counts(&self) -> Result<u64, DbErr> {
        let clocks_count = ClockInfos::find().count(self.pg_db.as_ref()).await;

        match clocks_count {
            Err(err) => {
                error!("Query clock_info counts error, err: {}", err);
                Err(err)
            }
            Ok(counts) => Ok(counts),
        }
    }
}
