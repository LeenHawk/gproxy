use super::*;

pub enum RecordBatch {
    Organizations(Vec<OrganizationInput>),
    Teams(Vec<TeamInput>),
    Users(Vec<UserInput>),
    UserKeys(Vec<UserKeyInput>),
    Providers(Vec<ProviderInput>),
    Credentials(Vec<CredentialInput>),
    Routes(Vec<RouteInput>),
    RouteMembers(Vec<RouteMemberInput>),
    Aliases(Vec<AliasInput>),
    ExposedModels(Vec<ExposedModelInput>),
    Quotas(Vec<QuotaInput>),
    PriceRules(Vec<PriceRuleInput>),
    PriceRates(Vec<PriceRateInput>),
    RoutingRules(Vec<RoutingRuleInput>),
    RuleSets(Vec<RuleSetInput>),
    Rules(Vec<RuleInput>),
    ProviderRuleSets(Vec<ProviderRuleSetInput>),
}
