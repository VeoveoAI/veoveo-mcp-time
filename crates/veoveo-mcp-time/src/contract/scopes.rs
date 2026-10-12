/// The OAuth scope that names Time's namespace. Scopes are coarse; Time's
/// permissions carry fine-grained access.
///
/// ```compile_fail
/// use veoveo_mcp_time::contract::TimeScope;
/// fn requires_time(_: TimeScope) {}
/// requires_time("time");
/// ```
/// ```compile_fail
/// use veoveo_mcp_time::contract::TimeScope;
/// #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, veoveo_types::Vocabulary)]
/// #[vocabulary(scope)]
/// enum OtherScope { #[vocabulary(rename = "other")]
/// Other
/// }
/// fn requires_time(_: TimeScope) {}
/// requires_time(OtherScope::Other);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, veoveo_types::Vocabulary)]
#[vocabulary(scope)]
pub enum TimeScope {
    #[vocabulary(rename = "time")]
    Time,
}
