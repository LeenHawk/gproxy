import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { RouteMember } from "@/api/routes";
import { EnabledToggle } from "@/components/enabled-toggle";
import { MemberNumberInput } from "@/components/routes/member-number-input";
import type { MemberChanges } from "@/components/routes/use-route-member-update";
import { Badge } from "@/components/ui/badge";

interface InlineProps {
  member: RouteMember;
  selecting: boolean;
  pending: boolean;
  onChange: (changes: MemberChanges) => void;
}

export function MemberIntegerField({ field, ...props }: InlineProps & { field: "tier" | "weight" }) {
  const { t } = useTranslation("routes");
  if (props.selecting) return props.member[field];
  return (
    <MemberNumberInput
      value={props.member[field]}
      label={t(field === "tier" ? "members.editTier" : "members.editWeight")}
      disabled={props.pending}
      onCommit={(value) => props.onChange({ [field]: value })}
    />
  );
}

export function MemberEnabledField({ member, selecting, pending, onChange }: InlineProps) {
  const { t } = useTranslation("routes");
  return selecting ? (
    <Badge variant={member.enabled ? "secondary" : "outline"}>
      {member.enabled ? t("status.enabled") : t("status.disabled")}
    </Badge>
  ) : (
    <EnabledToggle
      enabled={member.enabled}
      pending={pending}
      onToggle={(enabled) => onChange({ enabled })}
    />
  );
}

export function MemberCard({ providerName, actions, ...props }: InlineProps & {
  providerName: string;
  actions: ReactNode;
}) {
  const { t } = useTranslation("routes");
  const change = (changes: MemberChanges) => props.onChange(changes);
  return (
    <div className="grid gap-2">
      <div className="flex items-center justify-between">
        <span className="font-medium">{providerName}</span>
        <MemberEnabledField {...props} onChange={change} />
      </div>
      <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
        <span className="font-mono">{props.member.upstream_model_id}</span>
        {props.selecting ? (
          <><span>tier {props.member.tier}</span><span>w{props.member.weight}</span></>
        ) : (
          <>
            <label className="flex items-center gap-1">{t("members.tier")}<MemberIntegerField {...props} field="tier" onChange={change} /></label>
            <label className="flex items-center gap-1">{t("members.weight")}<MemberIntegerField {...props} field="weight" onChange={change} /></label>
          </>
        )}
      </div>
      {!props.selecting && actions}
    </div>
  );
}
