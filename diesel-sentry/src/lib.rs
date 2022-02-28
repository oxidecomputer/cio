//! Add Sentry tracing probes to Diesel connections.
//!
//! The `diesel-sentry` crate provides a diesel [`Connection`] that includes Sentry tracing points.
//! These are fired when a connection to the database is established and for each query.

use diesel::backend::Backend;
use diesel::connection::{AnsiTransactionManager, ConnectionGatWorkaround, SimpleConnection, TransactionManager};
use diesel::debug_query;
use diesel::expression::QueryMetadata;
use diesel::prelude::*;
use diesel::query_builder::{AsQuery, QueryFragment, QueryId};
use diesel::r2d2::R2D2Connection;
use std::ops::{Deref, DerefMut};
use uuid::Uuid;

// https://www.postgresql.org/docs/12/functions-info.html
// db.name
diesel::sql_function!(fn current_database() -> diesel::sql_types::Text);
// db.version
diesel::sql_function!(fn version() -> diesel::sql_types::Text);

#[derive(Queryable, Clone, Debug, PartialEq, Default)]
struct ConnectionInfo {
    current_database: String,
    version: String,
}

/// A [`Connection`] that includes Sentry tracing points.
#[derive(Debug)]
pub struct SentryConnection<C: Connection> {
    inner: C,
    info: ConnectionInfo,
}

impl<C: Connection> Deref for SentryConnection<C> {
    type Target = C;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<C: Connection> DerefMut for SentryConnection<C> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<C: Connection> SimpleConnection for SentryConnection<C> {
    #[tracing::instrument(
        fields(
            db.name=%self.info.current_database,
            db.system="postgresql",
            db.version=%self.info.version,
            otel.kind="client",
        ),
        skip(self, query),
    )]
    fn batch_execute(&mut self, query: &str) -> QueryResult<()> {
        let result = self.inner.batch_execute(query);
        result
    }
}

impl<'a, C: Connection> ConnectionGatWorkaround<'a, C::Backend> for SentryConnection<C> {
    type Cursor = <C as ConnectionGatWorkaround<'a, C::Backend>>::Cursor;
    type Row = <C as ConnectionGatWorkaround<'a, C::Backend>>::Row;
}

impl<C> Connection for SentryConnection<C>
where
    C: Connection<TransactionManager = AnsiTransactionManager>,
    <C::Backend as Backend>::QueryBuilder: Default,
{
    type Backend = C::Backend;
    type TransactionManager = C::TransactionManager;

    #[tracing::instrument(
        fields(
            db.name=tracing::field::Empty,
            db.system="postgresql",
            db.version=tracing::field::Empty,
            otel.kind="client",
        ),
        skip(database_url),
    )]
    fn establish(database_url: &str) -> ConnectionResult<Self> {
        tracing::debug!("establishing postgresql connection");
        let conn_id = Uuid::new_v4();
        let conn = C::establish(database_url);
        let inner = conn?;

        tracing::debug!("querying postgresql connection information");
        let info: ConnectionInfo = Default::default();
        /*diesel::select((current_database(), version()))
        .get_result(&mut conn?)
        .map_err(ConnectionError::CouldntSetupConfiguration)?;*/

        let span = tracing::Span::current();
        span.record("db.name", &info.current_database.as_str());
        span.record("db.version", &info.version.as_str());

        Ok(SentryConnection {
            inner,
            id: conn_id,
            info,
        })
    }

    #[tracing::instrument(
        fields(
            db.name=%self.info.current_database,
            db.system="postgresql",
            db.version=%self.info.version,
            otel.kind="client",
        ),
        skip(self, f),
    )]
    fn transaction<T, E, F>(&mut self, f: F) -> Result<T, E>
    where
        F: FnOnce(&mut Self) -> Result<T, E>,
        E: From<diesel::result::Error>,
    {
        let result = f(self);
        result
    }
    fn execute(&mut self, query: &str) -> QueryResult<usize> {
        let result = self.inner.execute(query);
        result
    }

    #[tracing::instrument(
        fields(
            db.name=%self.info.current_database,
            db.system="postgresql",
            db.version=%self.info.version,
            otel.kind="client",
        ),
        skip(self, source),
    )]
    fn load<T>(&mut self, source: T) -> QueryResult<<Self as ConnectionGatWorkaround<Self::Backend>>::Cursor>
    where
        T: AsQuery,
        T::Query: QueryFragment<Self::Backend> + QueryId,
        Self::Backend: QueryMetadata<T::SqlType>,
    {
        let query = source.as_query();
        let result = self.inner.load(query);
        result
    }

    #[tracing::instrument(
        fields(
            db.name=%self.info.current_database,
            db.system="postgresql",
            db.version=%self.info.version,
            otel.kind="client",
        ),
        skip(self, source),
    )]
    fn execute_returning_count<T>(&mut self, source: &T) -> QueryResult<usize>
    where
        T: QueryFragment<Self::Backend> + QueryId,
    {
        let result = self.inner.execute_returning_count(source);
        result
    }

    #[tracing::instrument(
        fields(
            db.name=%self.info.current_database,
            db.system="postgresql",
            db.version=%self.info.version,
            otel.kind="client",
        ),
        skip(self),
    )]
    fn transaction_state(&mut self) -> &mut <C::TransactionManager as TransactionManager<C>>::TransactionStateData {
        self.inner.transaction_state()
    }
}

impl<C> R2D2Connection for SentryConnection<C>
where
    C: R2D2Connection + Connection<TransactionManager = AnsiTransactionManager>,
    <C::Backend as Backend>::QueryBuilder: Default,
{
    fn ping(&mut self) -> QueryResult<()> {
        self.inner.ping()
    }
}
