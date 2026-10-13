//! Pages over rows that SQL already filtered and limited to one row past the page.
use crate::contract::{AdminPage, AdminPageLimit, CollectionPage};
use anyhow::{Context, Result};
#[cfg(feature = "mcp")]
use rmcp::ErrorData as McpError;

pub(crate) const PAGE_SIZE: usize = 100;

/// The fetch size for an administration page: its limit and one row of lookahead.
pub(crate) fn admin_fetch(limit: AdminPageLimit) -> u32 {
    limit.get() + 1
}

/// Keep the first `size` records and return the cursor after the last of them when a
/// further record exists.
fn take_page<R, C>(
    records: &mut Vec<R>,
    size: usize,
    position: impl Fn(&R) -> Result<C>,
) -> Result<Option<C>> {
    let has_more = records.len() > size;
    records.truncate(size);
    if has_more {
        Ok(Some(position(
            records.last().context("missing Time page cursor")?,
        )?))
    } else {
        Ok(None)
    }
}

pub(crate) fn page<R, T, C>(
    mut records: Vec<R>,
    position: impl Fn(&R) -> Result<C>,
    convert: impl Fn(R) -> Result<T>,
) -> Result<CollectionPage<T, C>> {
    let next_cursor = take_page(&mut records, PAGE_SIZE, position)?;
    Ok(CollectionPage {
        items: records.into_iter().map(convert).collect::<Result<_>>()?,
        limit: PAGE_SIZE,
        next_cursor,
    })
}

pub(crate) fn admin_page<R, T, C>(
    mut records: Vec<R>,
    limit: AdminPageLimit,
    position: impl Fn(&R) -> Result<C>,
    convert: impl Fn(R) -> Result<T>,
) -> Result<AdminPage<T, C>> {
    let size = usize::try_from(limit.get()).context("administration page limit")?;
    let next_cursor = take_page(&mut records, size, position)?;
    Ok(AdminPage {
        items: records.into_iter().map(convert).collect::<Result<_>>()?,
        next_cursor,
    })
}

#[cfg(feature = "mcp")]
pub(crate) fn query_error(error: anyhow::Error) -> McpError {
    if matches!(
        error.downcast_ref::<crate::persistence::PersistenceError>(),
        Some(crate::persistence::PersistenceError::InvalidTimeField { .. })
    ) {
        McpError::invalid_params("invalid Time collection cursor", None)
    } else {
        McpError::internal_error(error.to_string(), None)
    }
}
