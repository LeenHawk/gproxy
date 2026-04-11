import { useI18n } from "../../../app/i18n";
import { Button, Card } from "../../../components/ui";
import { RewriteRulesEditor } from "./SettingsEditors";
import type { ProviderFormState } from "./index";

export function RewriteRulesTab({
  form,
  onChange,
  onSave,
}: {
  form: ProviderFormState;
  onChange: (patch: Partial<ProviderFormState>) => void;
  onSave: () => void;
}) {
  const { t } = useI18n();

  const updateSetting = (key: string, value: string) => {
    onChange({ settings: { ...form.settings, [key]: value } });
  };

  return (
    <Card title={t("providers.rewrite.title")}>
      <RewriteRulesEditor
        value={form.settings.rewrite_rules ?? "[]"}
        onChange={(v) => updateSetting("rewrite_rules", v)}
        t={t}
      />
      <div className="mt-4">
        <Button onClick={onSave}>{t("common.save")}</Button>
      </div>
    </Card>
  );
}
