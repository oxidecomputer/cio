//! Add Sentry tracing probes to Diesel connections.
//!
//! The `diesel-sentry` crate provides a diesel [`Connection`] that includes Sentry tracing points.
//! These are fired when a connection to the database is established and for each query.

use std::sync::Arc;

use diesel::backend::Backend;
use diesel::connection::{AnsiTransactionManager, ConnectionGatWorkaround, SimpleConnection, TransactionManager};
use diesel::debug_query;
use diesel::expression::QueryMetadata;
use diesel::prelude::*;
use diesel::query_builder::{AsQuery, QueryFragment, QueryId};
use diesel::r2d2::R2D2Connection;
use sentry::Hub;
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
    id: Uuid,
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
        let mut txn = start_sentry_db_transaction("sql.query", query);
        let result = self.inner.batch_execute(query);
        txn.finish();
        result
    }
}

impl<'a, C: Connection> ConnectionGatWorkaround<'a, C::Backend> for SentryConnection<C> {
    type Cursor = <C as ConnectionGatWorkaround<'a, C::Backend>>::Cursor;
    type Row = <C as ConnectionGatWorkaround<'a, C::Backend>>::Row;
}

impl<C> Connection for SentryConnection<C>
where
    C: Connection<TransactionManager = AnsiTransactionManager, Backend = diesel::pg::Pg>,
    <C::Backend as Backend>::QueryBuilder: Default,
{
    type Backend = diesel::pg::Pg;
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
        let mut txn = start_sentry_db_transaction("connection", &conn_id.to_string());
        let conn = C::establish(database_url);
        let mut inner = conn?;

        tracing::debug!("querying postgresql connection information");
        let info: ConnectionInfo = diesel::select((current_database(), version()))
            .get_result(&mut inner)
            .map_err(ConnectionError::CouldntSetupConfiguration)?;

        let span = tracing::Span::current();
        span.record("db.name", &info.current_database.as_str());
        span.record("db.version", &info.version.as_str());

        tracing::debug!("db.name: {}", info.current_database);
        tracing::debug!("db.version: {}", info.version);

        txn.finish();

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
        let mut txn = start_sentry_db_transaction("transaction", &self.id.to_string());
        let result = f(self);
        txn.finish();
        result
    }
    fn execute(&mut self, query: &str) -> QueryResult<usize> {
        let mut txn = start_sentry_db_transaction("sql.query", query);
        let result = self.inner.execute(query);
        txn.finish();
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
        let mut txn = start_sentry_db_transaction("sql.query", &debug_query::<Self::Backend, _>(&query).to_string());
        let result = self.inner.load(query);
        txn.finish();
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
        let mut txn = start_sentry_db_transaction("sql.query", &debug_query::<Self::Backend, _>(&source).to_string());
        let result = self.inner.execute_returning_count(source);
        txn.finish();
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
    C: R2D2Connection + Connection<TransactionManager = AnsiTransactionManager, Backend = diesel::pg::Pg>,
    <C::Backend as Backend>::QueryBuilder: Default,
{
    fn ping(&mut self) -> QueryResult<()> {
        self.inner.ping()
    }
}

#[derive(Debug, Clone)]
pub struct SentryTransaction {
    transaction: sentry::TransactionOrSpan,
    parent_span: Option<sentry::TransactionOrSpan>,
    hub: Arc<sentry::Hub>,
}

fn start_sentry_db_transaction(op: &str, name: &str) -> SentryTransaction {
    // Create a new Sentry hub for every request.
    // Ensures the scope stays right.
    // The Clippy lint here is a false positive, the suggestion to write
    // `Hub::with(Hub::new_from_top)` does not compiles:
    //     143 |         Hub::with(Hub::new_from_top).into()
    //         |         ^^^^^^^^^ implementation of `std::ops::FnOnce` is not general enough
    #[allow(clippy::redundant_closure)]
    let hub = Arc::new(Hub::with(|hub| Hub::new_from_top(hub)));

    let trx_ctx = sentry::TransactionContext::new(name, &format!("db.{}", op));

    let transaction: sentry::TransactionOrSpan = sentry::start_transaction(trx_ctx).into();

    let mut trx = SentryTransaction {
        transaction: transaction.clone(),
        parent_span: None,
        hub: hub.clone(),
    };

    hub.configure_scope(|scope| {
        scope.add_event_processor(move |event| {
            // TODO: do we want to add information here like we did for the request event.
            Some(event)
        });

        trx.parent_span = scope.get_span();
        scope.set_span(Some(transaction.clone()));
    });

    trx
}

impl SentryTransaction {
    pub fn finish(&mut self) {
        if self.transaction.get_status().is_none() {
            // TODO: we should actually pass if there was an error or not here.
            self.transaction.set_status(sentry::protocol::SpanStatus::Ok);
        }
        self.transaction.clone().finish();

        if let Some(parent_span) = &self.parent_span {
            self.hub.configure_scope(|scope| {
                scope.set_span(Some(parent_span.clone()));
            });
        }
    }
}
