import { AbilityConstant, UIAbility, Want } from '@kit.AbilityKit';
import { window } from '@kit.ArkUI';
import { openGateway, privacyAccepted } from './GproxyPrivacy';

// This launcher imports no Tauri/native library and uses only local resources.
export default class PrivacyAbility extends UIAbility {
  private autoStartup: boolean = false;

  onCreate(_want: Want, launchParam: AbilityConstant.LaunchParam): void {
    this.autoStartup = launchParam.launchReason === AbilityConstant.LaunchReason.AUTO_STARTUP;
  }

  async onWindowStageCreate(stage: window.WindowStage): Promise<void> {
    if (privacyAccepted(this.context)) {
      await openGateway(this.context, this.autoStartup);
    } else if (this.autoStartup) {
      await this.context.terminateSelf();
    } else {
      await stage.loadContent('pages/GproxyPrivacyPage');
    }
  }
}
