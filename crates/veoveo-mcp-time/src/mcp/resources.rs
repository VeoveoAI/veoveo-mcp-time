//! Typed resource construction; caller scope is evaluated for every read.
use super::{TimeMcp, default_clock_policy, internal, invalid_params, not_found};
use crate::contract::*;
use rmcp::{ErrorData as McpError, RoleServer, service::RequestContext};
use veoveo_hub_sdk::{
    definition::ReadCollection,
    hosting::{ReadCache, ResourceBuilder, RoutePolicy, RoutesBuilder, hub_identity},
};
use veoveo_types::{LiveKeyset, Page, Pager};

type Context = RequestContext<RoleServer>;

pub(super) fn register(routes: RoutesBuilder<TimeMcp>) -> RoutesBuilder<TimeMcp> {
    let policy =
        || RoutePolicy::permission(TimePermission::AuthoritiesRead).expect("Time read permission");
    routes
        .singleton(
            ReadClockCurrent,
            ResourceBuilder::new("Current authoritative time")
                .reader(clock_current)
                .policy(policy())
                .cache(ReadCache::Private),
        )
        .singleton(
            ReadClockQuality,
            ResourceBuilder::new("Clock quality")
                .reader(clock_quality)
                .policy(policy())
                .cache(ReadCache::Private),
        )
        .singleton(
            ReadAuthoritiesCurrent,
            ResourceBuilder::new("Active time authorities")
                .reader(authorities_current)
                .policy(policy())
                .cache(ReadCache::Private),
        )
        .template::<ReadCalendar, _, _>(
            ResourceBuilder::new("Operational calendar")
                .reader(calendar)
                .policy(policy())
                .cache(ReadCache::Private),
        )
        .template::<ReadEpoch, _, _>(
            ResourceBuilder::new("Epoch")
                .reader(epoch)
                .policy(policy())
                .cache(ReadCache::Private),
        )
        .template::<ReadEpochVersion, _, _>(
            ResourceBuilder::new("Epoch version")
                .reader(epoch_version)
                .policy(policy())
                .cache(ReadCache::Private),
        )
        .template::<ReadEvent, _, _>(
            ResourceBuilder::new("Event")
                .reader(event)
                .policy(policy())
                .cache(ReadCache::Private),
        )
        .template::<ReadZone, _, _>(
            ResourceBuilder::new("IANA time zone")
                .reader(zone)
                .policy(policy())
                .cache(ReadCache::Private),
        )
        .template::<ReadAuthorityRelease, _, _>(
            ResourceBuilder::new("Time authority release")
                .reader(authority_release)
                .policy(policy())
                .cache(ReadCache::Private),
        )
        .template::<ReadBootstrapAuthority, _, _>(
            ResourceBuilder::new("Packaged authority")
                .reader(bootstrap_authority)
                .policy(policy())
                .cache(ReadCache::Private),
        )
        .collection_resource::<ReadCalendars, _, _>(
            Some(ReadCalendars),
            ResourceBuilder::new("Operational calendars")
                .reader(calendars)
                .policy(policy())
                .cache(ReadCache::Private),
        )
        .collection_resource::<ReadEpochs, _, _>(
            Some(ReadEpochs),
            ResourceBuilder::new("Epochs")
                .reader(epochs)
                .policy(policy())
                .cache(ReadCache::Private),
        )
        .collection_resource::<ReadEvents, _, _>(
            Some(ReadEvents),
            ResourceBuilder::new("Events")
                .reader(events)
                .policy(policy())
                .cache(ReadCache::Private),
        )
        .collection_resource::<ReadReleases, _, _>(
            Some(ReadReleases),
            ResourceBuilder::new("Acquired time authorities")
                .reader(releases)
                .policy(policy())
                .cache(ReadCache::Private),
        )
        .collection_resource::<ReadBootstrapAuthorities, _, _>(
            Some(ReadBootstrapAuthorities),
            ResourceBuilder::new("Packaged time authorities")
                .reader(bootstrap_authorities)
                .policy(policy())
                .cache(ReadCache::Private),
        )
}

async fn calendar(
    domain: TimeMcp,
    read: ReadCalendar,
    context: Context,
) -> Result<OperationalCalendar, McpError> {
    let scope = domain
        .state
        .scope(&hub_identity(&context)?)
        .await
        .map_err(internal)?;
    domain
        .state
        .catalog
        .calendar(&scope, &read.id, read.version)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("calendar version"))
}

async fn epoch(domain: TimeMcp, read: ReadEpoch, context: Context) -> Result<Epoch, McpError> {
    let scope = domain
        .state
        .scope(&hub_identity(&context)?)
        .await
        .map_err(internal)?;
    domain
        .state
        .catalog
        .epoch(&scope, &read.id)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("epoch"))
}

async fn epoch_version(
    domain: TimeMcp,
    read: ReadEpochVersion,
    context: Context,
) -> Result<Epoch, McpError> {
    let scope = domain
        .state
        .scope(&hub_identity(&context)?)
        .await
        .map_err(internal)?;
    domain
        .state
        .catalog
        .epoch_version(&scope, &read.id, read.version)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("epoch version"))
}

async fn event(domain: TimeMcp, read: ReadEvent, context: Context) -> Result<Event, McpError> {
    let scope = domain
        .state
        .scope(&hub_identity(&context)?)
        .await
        .map_err(internal)?;
    domain
        .state
        .catalog
        .event(&scope, &read.id)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("event"))
}

async fn calendars(
    domain: TimeMcp,
    read: ReadCollection<ReadCalendars>,
    context: Context,
) -> Result<Page<Calendars>, McpError> {
    let scope = domain
        .state
        .scope(&hub_identity(&context)?)
        .await
        .map_err(internal)?;
    domain
        .state
        .catalog
        .calendars_page(&scope, read.selection())
        .await
        .map_err(crate::index::query_error)
}

async fn epochs(
    domain: TimeMcp,
    read: ReadCollection<ReadEpochs>,
    context: Context,
) -> Result<Page<Epochs>, McpError> {
    let scope = domain
        .state
        .scope(&hub_identity(&context)?)
        .await
        .map_err(internal)?;
    domain
        .state
        .catalog
        .epochs_page(&scope, read.selection())
        .await
        .map_err(crate::index::query_error)
}

async fn events(
    domain: TimeMcp,
    read: ReadCollection<ReadEvents>,
    context: Context,
) -> Result<Page<Events>, McpError> {
    let scope = domain
        .state
        .scope(&hub_identity(&context)?)
        .await
        .map_err(internal)?;
    domain
        .state
        .catalog
        .events_page(&scope, read.selection())
        .await
        .map_err(crate::index::query_error)
}

async fn releases(
    domain: TimeMcp,
    read: ReadCollection<ReadReleases>,
    context: Context,
) -> Result<Page<Releases>, McpError> {
    let scope = domain
        .state
        .scope(&hub_identity(&context)?)
        .await
        .map_err(internal)?;
    domain
        .state
        .catalog
        .releases_page(&scope, read.selection())
        .await
        .map_err(crate::index::query_error)
}

async fn bootstrap_authorities(
    domain: TimeMcp,
    read: ReadCollection<ReadBootstrapAuthorities>,
    context: Context,
) -> Result<Page<BootstrapAuthorities>, McpError> {
    let scope = domain
        .state
        .scope(&hub_identity(&context)?)
        .await
        .map_err(internal)?;
    let selection = read.selection();
    let pager = Pager::<BootstrapAuthorities>::new(
        scope.identity.tenant_id,
        *selection.filters(),
        selection.limit(),
        LiveKeyset,
        selection.cursor(),
    )
    .map_err(invalid_params)?;
    pager
        .select(domain.state.authorities.bootstrap_references())
        .map_err(invalid_params)
}
async fn authority_release(
    domain: TimeMcp,
    read: ReadAuthorityRelease,
    context: Context,
) -> Result<TimeAuthorityReference, McpError> {
    let scope = domain
        .state
        .scope(&hub_identity(&context)?)
        .await
        .map_err(internal)?;
    let release = domain
        .state
        .catalog
        .release(&scope, &read.id)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("authority release"))?;
    domain
        .state
        .catalog
        .authority_reference(&scope, &release)
        .await
        .map_err(internal)
}
async fn bootstrap_authority(
    domain: TimeMcp,
    read: ReadBootstrapAuthority,
    context: Context,
) -> Result<TimeAuthorityReference, McpError> {
    domain
        .state
        .scope(&hub_identity(&context)?)
        .await
        .map_err(internal)?;
    domain
        .state
        .authorities
        .bootstrap_reference(&read.id)
        .ok_or_else(|| not_found("bootstrap authority"))
}
async fn clock_quality(
    domain: TimeMcp,
    _: ReadClockQuality,
    context: Context,
) -> Result<ClockQuality, McpError> {
    domain
        .state
        .scope(&hub_identity(&context)?)
        .await
        .map_err(internal)?;
    domain.state.clock.quality().await.map_err(internal)
}
async fn authorities_current(
    domain: TimeMcp,
    _: ReadAuthoritiesCurrent,
    context: Context,
) -> Result<EffectiveTimeAuthority, McpError> {
    let scope = domain
        .state
        .scope(&hub_identity(&context)?)
        .await
        .map_err(internal)?;
    let engine = domain.state.engine(&scope).await.map_err(internal)?;
    Ok(engine.authority().effective().clone())
}
async fn clock_current(
    domain: TimeMcp,
    _: ReadClockCurrent,
    context: Context,
) -> Result<ClockCurrent, McpError> {
    let scope = domain
        .state
        .scope(&hub_identity(&context)?)
        .await
        .map_err(internal)?;
    let engine = domain.state.engine(&scope).await.map_err(internal)?;
    let quality = domain.state.clock.quality().await.map_err(internal)?;
    let policy = domain
        .state
        .catalog
        .clock_policy(&scope)
        .await
        .map_err(internal)?
        .map(|value| value.0)
        .unwrap_or_else(default_clock_policy);
    let time = engine
        .resolve(&ResolveTimeRequest {
            expression: TimeExpressionValue::Rfc3339 {
                value: chrono::Utc::now().to_rfc3339(),
            }
            .build()
            .map_err(invalid_params)?,
            additional_uncertainty: quality.error_bound,
        })
        .map_err(invalid_params)?;
    let utc_offset_holds_until = engine.utc_offset_holds_until(&time).map_err(internal)?;
    Ok(ClockCurrent {
        time,
        utc_offset_holds_until,
        effective_policy: policy,
        clock_quality: quality,
    })
}
async fn zone(domain: TimeMcp, read: ReadZone, context: Context) -> Result<ZoneView, McpError> {
    let scope = domain
        .state
        .scope(&hub_identity(&context)?)
        .await
        .map_err(internal)?;
    let engine = domain.state.engine(&scope).await.map_err(internal)?;
    let now = engine
        .resolve(&ResolveTimeRequest {
            expression: TimeExpressionValue::Rfc3339 {
                value: chrono::Utc::now().to_rfc3339(),
            }
            .build()
            .map_err(invalid_params)?,
            additional_uncertainty: NanosecondBound::ZERO,
        })
        .map_err(invalid_params)?;
    let leap = engine.utc_offset_holds_until(&now).map_err(internal)?;
    let instant = now.into_instant();
    let offset_holds_until = engine
        .zone_offset_holds_until(&instant, read.zone.as_str(), leap)
        .map_err(invalid_params)?;
    let projection = engine
        .convert(&ConvertTimeRequest {
            instant,
            zone_ids: vec![read.zone.as_str().to_owned()],
            scales: Vec::new(),
        })
        .map_err(invalid_params)?;
    Ok(ZoneView {
        zone_id: read.zone.as_str().to_owned(),
        tzdb_release_id: engine.authority().binding().tzdb_release_id().clone(),
        current: projection.zoned.into_iter().next(),
        offset_holds_until,
    })
}
