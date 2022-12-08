use sqlx::{
    migrate::{MigrationSource, Migrator},
    Connection, Executor, PgConnection, PgPool,
};
use std::{path::Path, thread};
use tokio::runtime::Runtime;
use uuid::Uuid;

pub struct TestPostgres {
    server_url: String,
    dbname: String,
}

impl TestPostgres {
    pub fn new<S>(server_url: String, source: S) -> Self
    where
        S: MigrationSource<'static> + Send + Sync + 'static,
    {
        // create a random db name
        let dbname = format!("test_{}", Uuid::new_v4());
        let dbname_cloned = dbname.clone();
        let tpg = Self { server_url, dbname };
        // config.db.dbname = database_name.clone();
        let server_url = tpg.server_url();
        let url = tpg.url();

        thread::spawn(move || {
            let rt = Runtime::new().unwrap();
            // create database
            // server_url is not append a database name
            rt.block_on(async move {
                let mut conn = PgConnection::connect(&server_url).await.unwrap();
                conn.execute(format!(r#"CREATE Database "{}""#, dbname_cloned).as_str())
                    .await
                    .unwrap();
                let mut conn = PgConnection::connect(&url).await.unwrap();
                // migrate database
                let m = Migrator::new(source).await.unwrap();
                m.run(&mut conn).await.unwrap();
            });
        })
        .join()
        .expect("Thread panicked");

        tpg
    }

    pub fn server_url(&self) -> String {
        self.server_url.clone()
    }

    pub fn url(&self) -> String {
        format!("{}/{}", self.server_url, self.dbname)
    }

    pub async fn get_pool(&self) -> PgPool {
        PgPool::connect(&self.url()).await.unwrap()
    }
}

impl Drop for TestPostgres {
    fn drop(&mut self) {
        // server_url is not append a database name
        let url = self.server_url();
        let dbname = self.dbname.clone();
        println!("url is: {}", url);
        println!("database_name is: {}", dbname);
        thread::spawn(move || {
            let rt = Runtime::new().unwrap();
            // drop database
            rt.block_on(async move {
                let mut conn = PgConnection::connect(&url).await.unwrap();
                // terminate all connections
                sqlx::query(&format!(
                    r#"SELECT pg_terminate_backend(pid) FROM pg_stat_activity
                 WHERE pid <> pg_backend_pid() AND datname = '{}'"#,
                    dbname
                ))
                .execute(&mut conn)
                .await
                .expect("Terminate all other connections");

                // drop database
                conn.execute(format!(r#"DROP Database "{}""#, dbname).as_str())
                    .await
                    .expect("Error while querying the drop database");
            });
        })
        .join()
        .expect("failed to drop database");
    }
}

impl Default for TestPostgres {
    fn default() -> Self {
        Self::new(
            "postgres://tosei:tosei@localhost:5432".to_string(),
            Path::new("./migrations"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_postgres_can_create_and_drop() {
        let pg = TestPostgres::default();
        let pool = pg.get_pool().await;
        sqlx::query("insert into todos (title) values ('test')")
            .execute(&pool)
            .await
            .unwrap();

        let (id, title) = sqlx::query_as::<_, (i32, String)>("select id, title from todos")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(id, 1);
        assert_eq!(title, "test");
    }
}
