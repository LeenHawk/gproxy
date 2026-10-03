import { NativeAbility } from '@ohos-rs/ability';
import { AbilityConstant, Want } from '@kit.AbilityKit';
import window from '@ohos.window';
import webview from '@ohos.web.webview';
import native from 'libgproxy_host_tauri.so';
import { GproxyBackground } from './GproxyBackground';
import { privacyAccepted, STORE_DISTRIBUTION } from './GproxyPrivacy';

export default class EntryAbility extends NativeAbility {
  public moduleName: string = 'gproxy_host_tauri';
  public defaultPage: boolean = true;
  public mode: 'xcomponent' | 'webview' = 'webview';
  private background: GproxyBackground | undefined;
  private autoStartup: boolean = false;
  private privacyReady: boolean = false;

  async onCreate(want: Want, launchParam: AbilityConstant.LaunchParam): Promise<void> {
    this.autoStartup = launchParam.launchReason === AbilityConstant.LaunchReason.AUTO_STARTUP ||
      (STORE_DISTRIBUTION && want.parameters?.gproxyAutoStartup === true);
    this.privacyReady = privacyAccepted(this.context);
    if (!this.privacyReady) {
      if (!this.autoStartup) {
        await this.context.startAbility({ bundleName: 'com.leenhawk.gproxy.app', abilityName: 'PrivacyAbility' });
      }
      await this.context.terminateSelf();
      return;
    }
    this.background = new GproxyBackground(this.context, this.autoStartup);
    native.registerGproxyBackground(command => this.background!.dispatch(command));
    // The Rust handler is installed later, per WebView. ArkWeb also needs this
    // scheme declared before NativeAbility initializes the WebEngine.
    webview.WebviewController.customizeSchemes([{
      schemeName: 'gproxy-fonts',
      isSupportCORS: true,
      isSupportFetch: true,
      isStandard: true,
      isLocal: false,
      isDisplayIsolated: false,
      isSecure: false,
      isCspBypassing: false,
      isCodeCacheSupported: false,
    }]);
    await super.onCreate(want, launchParam);
  }

  async onWindowStageCreate(windowStage: window.WindowStage): Promise<void> {
    if (!this.privacyReady) return;
    await windowStage.getMainWindowSync().setWindowLayoutFullScreen(false);
    this.background?.attachWindow(windowStage.getMainWindowSync());
    await super.onWindowStageCreate(windowStage);
  }

  onNewWant(want: Want, launchParam: AbilityConstant.LaunchParam): void {
    this.autoStartup = launchParam.launchReason === AbilityConstant.LaunchReason.AUTO_STARTUP ||
      (STORE_DISTRIBUTION && want.parameters?.gproxyAutoStartup === true);
    if (!this.autoStartup) this.background?.manualOpen();
  }

  onForeground(): void {
    this.background?.foreground();
  }

  async onDestroy(): Promise<void> {
    if (!this.privacyReady) return;
    try { await this.background?.destroy(); }
    finally { await super.onDestroy(); native.gproxyShutdown(); }
  }
}
