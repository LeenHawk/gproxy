import { useState, type Dispatch, type DragEvent, type SetStateAction } from "react";

import { Button, Input, Label, Select, TextArea } from "../../../components/ui";
import {
  BUILD_UA_ARCH,
  BUILD_UA_OS,
  DEFAULT_GPROXY_USER_AGENT_DRAFT,
  cacheBreakpointRulesDraftToStoredString,
  normalizeCacheBreakpointRulesDraft,
  type CacheBreakpointRuleDraft,
} from "./channels/shared";
import {
  CLAUDE_AGENT_SDK_PRELUDE_TEXT,
  CLAUDE_CODE_SYSTEM_PRELUDE_TEXT,
  OPERATION_OPTIONS,
  PROTOCOL_OPTIONS,
  type DispatchMode,
  type DispatchRuleDraft,
  type ProviderFormState,
  defaultChannelSettingsDraft,
  defaultDispatchRulesForChannel
} from "./index";

type TranslateFn = (key: string, params?: Record<string, string | number>) => string;

export function ConfigTab({
  providerForm,
  setProviderForm,
  channelOptions,
  showCodexOAuthIssuer,
  showOAuthTriplet,
  showVertexOAuthToken,
  showClaudeCodeSettings,
  showClaudeTopLevelCacheControl,
  showCustomMaskTable,
  addDispatchRule,
  updateDispatchRule,
  removeDispatchRule,
  isCreatingProvider,
  onCancelCreate,
  onSave,
  t
}: {
  providerForm: ProviderFormState;
  setProviderForm: Dispatch<SetStateAction<ProviderFormState>>;
  channelOptions: Array<{ value: string; label: string }>;
  showCodexOAuthIssuer: boolean;
  showOAuthTriplet: boolean;
  showVertexOAuthToken: boolean;
  showClaudeCodeSettings: boolean;
  showClaudeTopLevelCacheControl: boolean;
  showCustomMaskTable: boolean;
  addDispatchRule: () => void;
  updateDispatchRule: (id: string, patch: Partial<Omit<DispatchRuleDraft, "id">>) => void;
  removeDispatchRule: (id: string) => void;
  isCreatingProvider: boolean;
  onCancelCreate: () => void;
  onSave: () => void;
  t: TranslateFn;
}) {
  const [dispatchExpanded, setDispatchExpanded] = useState(false);
  const [cacheBreakpointsExpanded, setCacheBreakpointsExpanded] = useState(false);
  const maxDispatchRowsWhenCollapsed = 3;
  const hasMoreDispatchRules =
    providerForm.dispatchRules.length > maxDispatchRowsWhenCollapsed;
  const visibleDispatchRules =
    !dispatchExpanded && hasMoreDispatchRules
      ? providerForm.dispatchRules.slice(0, maxDispatchRowsWhenCollapsed)
      : providerForm.dispatchRules;
  const preludeTemplates = [
    {
      key: "none",
      label: t("common.none"),
      value: ""
    },
    {
      key: "code",
      label: t("providers.prelude.template.code"),
      value: CLAUDE_CODE_SYSTEM_PRELUDE_TEXT
    },
    {
      key: "agent",
      label: t("providers.prelude.template.agent"),
      value: CLAUDE_AGENT_SDK_PRELUDE_TEXT
    }
  ] as const;
  const geminiCliTemplate = `GeminiCLI/0.30.0/gemini-2.5-pro (${BUILD_UA_OS}; ${BUILD_UA_ARCH})`;
  const userAgentTemplateOptions = [
    { value: "", label: t("providers.uaTemplate.placeholder") },
    {
      value: DEFAULT_GPROXY_USER_AGENT_DRAFT,
      label: t("providers.uaTemplate.channel.gproxy")
    },
    { value: "codex_vscode/0.99.0", label: t("providers.uaTemplate.channel.codex") },
    { value: "claude-code/2.1.62", label: t("providers.uaTemplate.channel.claudecode") },
    { value: geminiCliTemplate, label: t("providers.uaTemplate.channel.geminicli") },
    {
      value: "antigravity/1.15.8 (Windows; AMD64)",
      label: t("providers.uaTemplate.channel.antigravity")
    },
    {
      value: "Visual Studio Code/1.99.0",
      label: t("providers.uaTemplate.ide.vscode")
    },
    {
      value: "IntelliJIdea/2025.3.2",
      label: t("providers.uaTemplate.ide.intellij")
    },
    { value: "PyCharm/2024.5.2", label: t("providers.uaTemplate.ide.pycharm") },
    {
      value: "Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)",
      label: t("providers.uaTemplate.bot.googlebot")
    },
    {
      value: "Mozilla/5.0 (compatible; bingbot/2.0; +http://www.bing.com/bingbot.htm)",
      label: t("providers.uaTemplate.bot.bingbot")
    }
  ];
  const cacheBreakpointRules = normalizeCacheBreakpointRulesDraft(
    providerForm.settings.cache_breakpoints ?? "[]"
  );
  const cacheBreakpointSlots: Array<CacheBreakpointRuleDraft | null> = Array.from(
    { length: 4 },
    (_, idx) => cacheBreakpointRules[idx] ?? null
  );
  const cacheRuleDragMime = "application/x-gproxy-cache-rule";

  const setCacheBreakpointRules = (nextRules: CacheBreakpointRuleDraft[]) => {
    setProviderForm((prev) => ({
      ...prev,
      settings: {
        ...prev.settings,
        cache_breakpoints: cacheBreakpointRulesDraftToStoredString(nextRules)
      }
    }));
  };

  const normalizeRule = (rule: CacheBreakpointRuleDraft): CacheBreakpointRuleDraft => {
    const next: CacheBreakpointRuleDraft = {
      target: rule.target,
      position: rule.position,
      index: Number.isFinite(rule.index) ? Math.max(1, Math.trunc(rule.index)) : 1,
      ttl: rule.ttl
    };
    if (next.target === "top_level") {
      next.position = "nth";
      next.index = 1;
    }
    return next;
  };

  const setCacheBreakpointSlots = (nextSlots: Array<CacheBreakpointRuleDraft | null>) => {
    const nextRules = nextSlots
      .filter((rule): rule is CacheBreakpointRuleDraft => rule !== null)
      .map(normalizeRule);
    setCacheBreakpointRules(nextRules);
  };

  const defaultSlotRule = (): CacheBreakpointRuleDraft => ({
    target: "messages",
    position: "nth",
    index: 1,
    ttl: "auto"
  });

  const replaceCacheBreakpointSlot = (idx: number, nextRule: CacheBreakpointRuleDraft | null) => {
    if (idx < 0 || idx >= cacheBreakpointSlots.length) {
      return;
    }
    const nextSlots = [...cacheBreakpointSlots];
    nextSlots[idx] = nextRule ? normalizeRule(nextRule) : null;
    setCacheBreakpointSlots(nextSlots);
  };

  const updateCacheBreakpointSlot = (
    idx: number,
    patch: Partial<CacheBreakpointRuleDraft>
  ) => {
    if (idx < 0 || idx >= cacheBreakpointSlots.length) {
      return;
    }
    const current = cacheBreakpointSlots[idx] ?? defaultSlotRule();
    replaceCacheBreakpointSlot(idx, { ...current, ...patch });
  };

  const applyRecommendedTemplate = () => {
    setCacheBreakpointSlots([
      { target: "system", position: "last_nth", index: 1, ttl: "auto" },
      { target: "messages", position: "last_nth", index: 11, ttl: "auto" },
      { target: "messages", position: "last_nth", index: 2, ttl: "auto" },
      { target: "messages", position: "last_nth", index: 1, ttl: "auto" }
    ]);
  };

  const cardExamples: Array<{ id: string; label: string; rule: CacheBreakpointRuleDraft }> = [
    {
      id: "example-top-level",
      label: t("providers.cacheBreakpoints.card.top_level"),
      rule: { target: "top_level", position: "nth", index: 1, ttl: "auto" }
    },
    {
      id: "example-tools",
      label: t("providers.cacheBreakpoints.card.tools"),
      rule: { target: "tools", position: "nth", index: 1, ttl: "auto" }
    },
    {
      id: "example-system",
      label: t("providers.cacheBreakpoints.card.system"),
      rule: { target: "system", position: "nth", index: 1, ttl: "auto" }
    },
    {
      id: "example-messages",
      label: t("providers.cacheBreakpoints.card.messages"),
      rule: { target: "messages", position: "nth", index: 1, ttl: "auto" }
    }
  ];

  const onRuleCardDragStart = (
    event: DragEvent<HTMLDivElement>,
    rule: CacheBreakpointRuleDraft
  ) => {
    event.dataTransfer.effectAllowed = "copy";
    const payload = JSON.stringify(rule);
    event.dataTransfer.setData(cacheRuleDragMime, payload);
    event.dataTransfer.setData("text/plain", payload);
  };

  const onRuleSlotDragOver = (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    event.dataTransfer.dropEffect = "copy";
  };

  const parseDroppedRule = (raw: string): CacheBreakpointRuleDraft | null => {
    try {
      const value = JSON.parse(raw) as Partial<CacheBreakpointRuleDraft>;
      const target = value.target;
      const position = value.position;
      const ttl = value.ttl;
      const index = value.index;
      if (
        target !== "top_level" &&
        target !== "tools" &&
        target !== "system" &&
        target !== "messages"
      ) {
        return null;
      }
      if (position !== "nth" && position !== "last_nth") {
        return null;
      }
      if (ttl !== "auto" && ttl !== "5m" && ttl !== "1h") {
        return null;
      }
      if (typeof index !== "number") {
        return null;
      }
      return normalizeRule({
        target,
        position,
        ttl,
        index
      });
    } catch {
      return null;
    }
  };

  const onRuleSlotDrop = (slotIdx: number, event: DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    const raw =
      event.dataTransfer.getData(cacheRuleDragMime) || event.dataTransfer.getData("text/plain");
    if (!raw) {
      return;
    }
    const droppedRule = parseDroppedRule(raw);
    if (!droppedRule) {
      return;
    }
    replaceCacheBreakpointSlot(slotIdx, droppedRule);
  };

  const renderEyeIcon = (shown: boolean) => (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      className="h-4 w-4"
      aria-hidden="true"
    >
      <path d="M2 12s3.5-6 10-6 10 6 10 6-3.5 6-10 6-10-6-10-6Z" />
      <circle cx="12" cy="12" r="2.8" />
      {shown ? null : <path d="M4 20L20 4" />}
    </svg>
  );

  return (
    <div className="space-y-4">
      <div className="grid gap-3 md:grid-cols-2">
        <div>
          <Label>{t("field.id")}</Label>
          <Input value={providerForm.id} onChange={() => {}} disabled />
        </div>
        <div>
          <Label>{t("field.name")}</Label>
          <Input
            value={providerForm.name}
            onChange={(v) => setProviderForm((p) => ({ ...p, name: v }))}
          />
        </div>
        <div>
          <Label>{t("field.channel")}</Label>
          <Select
            value={providerForm.channel}
            onChange={(value) =>
              setProviderForm((prev) => ({
                ...prev,
                channel: value,
                settings: defaultChannelSettingsDraft(value),
                dispatchRules: defaultDispatchRulesForChannel(value)
              }))
            }
            options={channelOptions}
          />
        </div>
        {showClaudeTopLevelCacheControl ? (
          <div>
            <Label>{t("field.cache_breakpoints")}</Label>
            {!cacheBreakpointsExpanded ? (
              <button
                type="button"
                className="input flex w-full items-center justify-center gap-2 text-center"
                onClick={() => setCacheBreakpointsExpanded(true)}
                title={t("providers.cacheBreakpoints.eye.open")}
                aria-label={t("providers.cacheBreakpoints.eye.open")}
              >
                <span className="inline-flex h-6 w-6 items-center justify-center rounded-md border border-border bg-panel-muted text-muted">
                  {renderEyeIcon(false)}
                </span>
                <span className="text-sm text-muted">
                  {t("providers.cacheBreakpoints.compact", { count: cacheBreakpointRules.length })}
                </span>
              </button>
            ) : (
              <div className="cache-breakpoints-panel rounded p-3">
                <div className="flex items-center justify-between gap-2">
                  <div className="text-xs text-muted">{t("providers.cacheBreakpoints.hint")}</div>
                  <div className="flex items-center gap-2">
                    <Button variant="neutral" onClick={applyRecommendedTemplate}>
                      {t("providers.cacheBreakpoints.template")}
                    </Button>
                    <button
                      type="button"
                      className="inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-md border border-border bg-panel-muted text-muted transition hover:text-text"
                      onClick={() => setCacheBreakpointsExpanded(false)}
                      title={t("providers.cacheBreakpoints.eye.close")}
                      aria-label={t("providers.cacheBreakpoints.eye.close")}
                    >
                      {renderEyeIcon(true)}
                    </button>
                  </div>
                </div>
                <div className="mt-3">
                  <div className="mb-2 text-xs text-muted">
                    {t("providers.cacheBreakpoints.examples")}
                  </div>
                  <div className="grid grid-cols-2 gap-2 xl:grid-cols-4">
                    {cardExamples.map((item) => (
                      <div
                        key={item.id}
                        draggable
                        onDragStart={(event) => onRuleCardDragStart(event, item.rule)}
                        className="cache-breakpoint-reference-card flex min-h-[40px] cursor-grab items-center justify-center rounded px-2 py-2 text-center text-sm active:cursor-grabbing"
                      >
                        {item.label}
                      </div>
                    ))}
                  </div>
                </div>
                <div className="mt-3">
                  <div className="mb-2 text-xs text-muted">{t("providers.cacheBreakpoints.slots")}</div>
                  <div className="grid gap-2 sm:grid-cols-2">
                    {cacheBreakpointSlots.map((rule, idx) => (
                      <div
                        key={`cache-slot-${idx + 1}`}
                        onDragOver={onRuleSlotDragOver}
                        onDrop={(event) => onRuleSlotDrop(idx, event)}
                        className="cache-breakpoint-slot rounded p-2"
                      >
                        <div className="mb-2 flex items-center justify-between">
                          <div className="text-xs text-muted">
                            {t("providers.cacheBreakpoints.slot.title", { index: idx + 1 })}
                          </div>
                          <Button variant="neutral" onClick={() => replaceCacheBreakpointSlot(idx, null)}>
                            {t("providers.cacheBreakpoints.clear")}
                          </Button>
                        </div>
                        {rule ? (
                          <div className="space-y-2">
                            <Select
                              value={rule.target}
                              onChange={(value) =>
                                updateCacheBreakpointSlot(idx, {
                                  target: value as CacheBreakpointRuleDraft["target"]
                                })
                              }
                              options={[
                                { value: "top_level", label: t("providers.cacheBreakpoints.target.top_level") },
                                { value: "tools", label: t("providers.cacheBreakpoints.target.tools") },
                                { value: "system", label: t("providers.cacheBreakpoints.target.system") },
                                { value: "messages", label: t("providers.cacheBreakpoints.target.messages") }
                              ]}
                            />
                            {rule.target === "top_level" ? null : (
                              <div className="grid grid-cols-2 gap-2">
                                <Select
                                  value={rule.position}
                                  onChange={(value) =>
                                    updateCacheBreakpointSlot(idx, {
                                      position: value as CacheBreakpointRuleDraft["position"]
                                    })
                                  }
                                  options={[
                                    { value: "nth", label: t("providers.cacheBreakpoints.position.nth") },
                                    {
                                      value: "last_nth",
                                      label: t("providers.cacheBreakpoints.position.last_nth")
                                    }
                                  ]}
                                />
                                <Input
                                  value={String(rule.index)}
                                  onChange={(value) =>
                                    updateCacheBreakpointSlot(idx, {
                                      index: Math.max(1, Number.parseInt(value, 10) || 1)
                                    })
                                  }
                                />
                              </div>
                            )}
                            <Select
                              value={rule.ttl}
                              onChange={(value) =>
                                updateCacheBreakpointSlot(idx, {
                                  ttl: value as CacheBreakpointRuleDraft["ttl"]
                                })
                              }
                              options={[
                                { value: "auto", label: t("providers.cacheBreakpoints.ttl.auto") },
                                { value: "5m", label: t("providers.cacheBreakpoints.ttl.5m") },
                                { value: "1h", label: t("providers.cacheBreakpoints.ttl.1h") }
                              ]}
                            />
                          </div>
                        ) : (
                          <div className="cache-breakpoint-empty flex min-h-[124px] items-center justify-center rounded px-2 text-center text-sm">
                            {t("providers.cacheBreakpoints.slot.empty")}
                          </div>
                        )}
                      </div>
                    ))}
                  </div>
                </div>
              </div>
            )}
          </div>
        ) : null}
        <div>
          <Label>{t("field.credential_round_robin_enabled")}</Label>
          <Select
            value={providerForm.credentialRoundRobinEnabled ? "true" : "false"}
            onChange={(value) =>
              setProviderForm((prev) => {
                const roundRobinEnabled = value === "true";
                return {
                  ...prev,
                  credentialRoundRobinEnabled: roundRobinEnabled,
                  credentialCacheAffinityEnabled: roundRobinEnabled
                    ? prev.credentialCacheAffinityEnabled
                    : false
                };
              })
            }
            options={[
              { value: "false", label: t("common.disabled") },
              { value: "true", label: t("common.enabled") }
            ]}
          />
        </div>
        <div>
          <Label>{t("field.credential_cache_affinity_enabled")}</Label>
          <Select
            value={
              providerForm.credentialRoundRobinEnabled &&
              providerForm.credentialCacheAffinityEnabled
                ? "true"
                : "false"
            }
            onChange={(value) =>
              setProviderForm((prev) => ({
                ...prev,
                credentialCacheAffinityEnabled:
                  prev.credentialRoundRobinEnabled && value === "true"
              }))
            }
            options={[
              { value: "false", label: t("common.disabled") },
              { value: "true", label: t("common.enabled") }
            ]}
            disabled={!providerForm.credentialRoundRobinEnabled}
          />
        </div>
        <div className="md:col-span-2">
          <Label>{t("field.base_url")}</Label>
          <Input
            value={providerForm.settings.base_url ?? ""}
            onChange={(value) =>
              setProviderForm((prev) => ({
                ...prev,
                settings: { ...prev.settings, base_url: value }
              }))
            }
          />
        </div>
        <div className="md:col-span-2 rounded-lg border border-border p-3">
          <Label>{t("field.user_agent")}</Label>
          <div className="space-y-2">
            <Select
              value=""
              onChange={(value) => {
                if (!value) {
                  return;
                }
                setProviderForm((prev) => ({
                  ...prev,
                  settings: { ...prev.settings, user_agent: value }
                }));
              }}
              options={userAgentTemplateOptions}
            />
            <Input
              value={providerForm.settings.user_agent ?? ""}
              onChange={(value) =>
                setProviderForm((prev) => ({
                  ...prev,
                  settings: { ...prev.settings, user_agent: value }
                }))
              }
            />
          </div>
        </div>
        {showCustomMaskTable ? (
          <div className="md:col-span-2 rounded-lg border border-border p-3">
            <div className="mb-2 text-xs font-semibold uppercase tracking-[0.08em] text-muted">
              custom
            </div>
            <Label>{t("field.custom_mask_table")}</Label>
            <TextArea
              rows={8}
              value={providerForm.settings.mask_table ?? ""}
              onChange={(value) =>
                setProviderForm((prev) => ({
                  ...prev,
                  settings: { ...prev.settings, mask_table: value }
                }))
              }
            />
            <p className="mt-2 text-xs text-muted">{t("providers.custom.maskHint")}</p>
          </div>
        ) : null}
        {showCodexOAuthIssuer ? (
          <div className="md:col-span-2 rounded-lg border border-border p-3">
            <div className="mb-2 text-xs font-semibold uppercase tracking-[0.08em] text-muted">
              {t("providers.section.oauth")}
            </div>
            <Label>{t("field.oauth_issuer_url")}</Label>
            <Input
              value={providerForm.settings.oauth_issuer_url ?? ""}
              onChange={(value) =>
                setProviderForm((prev) => ({
                  ...prev,
                  settings: { ...prev.settings, oauth_issuer_url: value }
                }))
              }
            />
          </div>
        ) : null}
        {showOAuthTriplet ? (
          <div className="md:col-span-2 rounded-lg border border-border p-3">
            <div className="mb-2 text-xs font-semibold uppercase tracking-[0.08em] text-muted">
              {t("providers.section.oauth")}
            </div>
            <div className="grid gap-3 md:grid-cols-3">
              <div>
                <Label>{t("field.oauth_authorize_url")}</Label>
                <Input
                  value={providerForm.settings.oauth_authorize_url ?? ""}
                  onChange={(value) =>
                    setProviderForm((prev) => ({
                      ...prev,
                      settings: { ...prev.settings, oauth_authorize_url: value }
                    }))
                  }
                />
              </div>
              <div>
                <Label>{t("field.oauth_token_url")}</Label>
                <Input
                  value={providerForm.settings.oauth_token_url ?? ""}
                  onChange={(value) =>
                    setProviderForm((prev) => ({
                      ...prev,
                      settings: { ...prev.settings, oauth_token_url: value }
                    }))
                  }
                />
              </div>
              <div>
                <Label>{t("field.oauth_userinfo_url")}</Label>
                <Input
                  value={providerForm.settings.oauth_userinfo_url ?? ""}
                  onChange={(value) =>
                    setProviderForm((prev) => ({
                      ...prev,
                      settings: { ...prev.settings, oauth_userinfo_url: value }
                    }))
                  }
                />
              </div>
            </div>
          </div>
        ) : null}
        {showVertexOAuthToken ? (
          <div className="md:col-span-2 rounded-lg border border-border p-3">
            <div className="mb-2 text-xs font-semibold uppercase tracking-[0.08em] text-muted">
              {t("providers.section.oauth")}
            </div>
            <Label>{t("field.oauth_token_url")}</Label>
            <Input
              value={providerForm.settings.oauth_token_url ?? ""}
              onChange={(value) =>
                setProviderForm((prev) => ({
                  ...prev,
                  settings: { ...prev.settings, oauth_token_url: value }
                }))
              }
            />
          </div>
        ) : null}
        {showClaudeCodeSettings ? (
          <div className="md:col-span-2 rounded-lg border border-border p-3">
            <div className="mb-2 text-xs font-semibold uppercase tracking-[0.08em] text-muted">
              claudecode
            </div>
            <div className="grid gap-3 md:grid-cols-2">
              <div>
                <Label>{t("field.claudecode_ai_base_url")}</Label>
                <Input
                  value={providerForm.settings.claudecode_ai_base_url ?? ""}
                  onChange={(value) =>
                    setProviderForm((prev) => ({
                      ...prev,
                      settings: { ...prev.settings, claudecode_ai_base_url: value }
                    }))
                  }
                />
              </div>
              <div>
                <Label>{t("field.claudecode_platform_base_url")}</Label>
                <Input
                  value={providerForm.settings.claudecode_platform_base_url ?? ""}
                  onChange={(value) =>
                    setProviderForm((prev) => ({
                      ...prev,
                      settings: { ...prev.settings, claudecode_platform_base_url: value }
                    }))
                  }
                />
              </div>
              <div className="md:col-span-2">
                <Label>{t("field.claudecode_prelude_text")}</Label>
                <TextArea
                  rows={4}
                  value={providerForm.settings.claudecode_prelude_text ?? ""}
                  onChange={(value) =>
                    setProviderForm((prev) => ({
                      ...prev,
                      settings: { ...prev.settings, claudecode_prelude_text: value }
                    }))
                  }
                />
                <div className="mt-2 flex flex-wrap gap-2">
                  {preludeTemplates.map((template) => (
                    <Button
                      key={template.key}
                      variant={
                        (providerForm.settings.claudecode_prelude_text ?? "") === template.value
                          ? "primary"
                          : "neutral"
                      }
                      onClick={() =>
                        setProviderForm((prev) => ({
                          ...prev,
                          settings: {
                            ...prev.settings,
                            claudecode_prelude_text: template.value
                          }
                        }))
                      }
                    >
                      {template.label}
                    </Button>
                  ))}
                </div>
                <p className="mt-2 text-xs text-muted">{t("providers.prelude.hint")}</p>
              </div>
            </div>
          </div>
        ) : null}
      </div>

      <div className="space-y-3">
        <div className="flex items-center justify-between">
          <Label>{t("field.dispatch_rules")}</Label>
          <div className="flex items-center gap-2">
            {hasMoreDispatchRules ? (
              <Button variant="neutral" onClick={() => setDispatchExpanded((value) => !value)}>
                {dispatchExpanded
                  ? t("providers.dispatch.collapse")
                  : t("providers.dispatch.expand")}
              </Button>
            ) : null}
            <Button variant="neutral" onClick={addDispatchRule}>
              {t("providers.addRule")}
            </Button>
          </div>
        </div>
        <div className="space-y-3">
          {visibleDispatchRules.map((rule) => (
            <div key={rule.id} className="provider-card space-y-2">
              <div className="grid gap-2 md:grid-cols-6">
                <div>
                  <Label>{t("field.src_op")}</Label>
                  <Select
                    value={rule.srcOperation}
                    onChange={(value) => updateDispatchRule(rule.id, { srcOperation: value })}
                    options={OPERATION_OPTIONS.map((item) => ({ value: item, label: item }))}
                  />
                </div>
                <div>
                  <Label>{t("field.src_proto")}</Label>
                  <Select
                    value={rule.srcProtocol}
                    onChange={(value) => updateDispatchRule(rule.id, { srcProtocol: value })}
                    options={PROTOCOL_OPTIONS.map((item) => ({ value: item, label: item }))}
                  />
                </div>
                <div>
                  <Label>{t("field.mode")}</Label>
                  <Select
                    value={rule.mode}
                    onChange={(value) =>
                      updateDispatchRule(rule.id, { mode: value as DispatchMode })
                    }
                    options={[
                      { value: "passthrough", label: t("providers.mode.passthrough") },
                      { value: "transform", label: t("providers.mode.transform") },
                      { value: "local", label: t("providers.mode.local") },
                      { value: "unsupported", label: t("providers.mode.unsupported") }
                    ]}
                  />
                </div>
                <div>
                  <Label>{t("field.dst_op")}</Label>
                  <Select
                    value={rule.dstOperation}
                    onChange={(value) => updateDispatchRule(rule.id, { dstOperation: value })}
                    options={OPERATION_OPTIONS.map((item) => ({ value: item, label: item }))}
                    disabled={rule.mode !== "transform"}
                  />
                </div>
                <div>
                  <Label>{t("field.dst_proto")}</Label>
                  <Select
                    value={rule.dstProtocol}
                    onChange={(value) => updateDispatchRule(rule.id, { dstProtocol: value })}
                    options={PROTOCOL_OPTIONS.map((item) => ({ value: item, label: item }))}
                    disabled={rule.mode !== "transform"}
                  />
                </div>
                <div className="flex items-end">
                  <Button variant="danger" onClick={() => removeDispatchRule(rule.id)}>
                    {t("common.delete")}
                  </Button>
                </div>
              </div>
            </div>
          ))}
          {providerForm.dispatchRules.length > visibleDispatchRules.length ? (
            <p className="text-xs text-muted">
              {t("providers.dispatch.visibleHint", {
                shown: visibleDispatchRules.length,
                total: providerForm.dispatchRules.length
              })}
            </p>
          ) : null}
        </div>
      </div>

      <div className="flex flex-wrap gap-2">
        <Button onClick={onSave}>{t("providers.save")}</Button>
        {isCreatingProvider ? (
          <Button variant="neutral" onClick={onCancelCreate}>
            {t("common.cancel")}
          </Button>
        ) : null}
      </div>
    </div>
  );
}
