import { common } from '@kit.AbilityKit';
import { preferences } from '@kit.ArkData';

export const PRIVACY_VERSION: string = '__PRIVACY_VERSION__';
export const STORE_DISTRIBUTION: boolean = __STORE_DISTRIBUTION__;

export function privacyAccepted(context: common.UIAbilityContext): boolean {
  if (!STORE_DISTRIBUTION) return true;
  return preferences.getPreferencesSync(context, { name: 'gproxy_privacy' })
    .getSync('acceptedVersion', '') === PRIVACY_VERSION;
}

export async function acceptPrivacy(context: common.UIAbilityContext): Promise<void> {
  const store = preferences.getPreferencesSync(context, { name: 'gproxy_privacy' });
  store.putSync('acceptedVersion', PRIVACY_VERSION);
  await store.flush();
}

export async function openGateway(context: common.UIAbilityContext, autoStartup: boolean = false): Promise<void> {
  await context.startAbility({ bundleName: 'dev.gproxy.desktop', abilityName: 'EntryAbility',
    parameters: { gproxyAutoStartup: autoStartup } });
  await context.terminateSelf();
}
