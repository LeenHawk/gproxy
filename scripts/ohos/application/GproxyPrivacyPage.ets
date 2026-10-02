import { common } from '@kit.AbilityKit';
import { i18n } from '@kit.LocalizationKit';
import { util } from '@kit.ArkTS';
import { acceptPrivacy, openGateway } from '../entryability/GproxyPrivacy';

@Entry
@Component
struct GproxyPrivacyPage {
  @State policy: string = '';
  @State busy: boolean = false;
  @State failure: string = '';
  private chinese: boolean = i18n.System.getSystemLanguage().startsWith('zh');

  async aboutToAppear(): Promise<void> {
    const context = getContext(this) as common.UIAbilityContext;
    const data = await context.resourceManager.getRawFileContent(
      this.chinese ? 'privacy-zh-CN.txt' : 'privacy-en-US.txt');
    this.policy = util.TextDecoder.create('utf-8').decodeToString(data);
  }

  private async accept(): Promise<void> {
    this.busy = true;
    try {
      const context = getContext(this) as common.UIAbilityContext;
      await acceptPrivacy(context);
      await openGateway(context);
    } catch (_) {
      this.failure = this.chinese ? '无法打开应用，请重试。' : 'Could not open the application. Please retry.';
      this.busy = false;
    }
  }

  build() {
    Column({ space: 12 }) {
      Scroll() {
        Text(this.policy).fontSize(16).width('100%').copyOption(CopyOptions.LocalDevice)
      }.layoutWeight(1).width('100%')
      if (this.failure) {
        Text(this.failure).fontSize(16)
      }
      Button(this.chinese ? '同意并继续' : 'Agree and continue')
        .width('100%').enabled(!this.busy && this.policy.length > 0)
        .onClick(() => { this.accept(); })
      Button(this.chinese ? '不同意并退出' : 'Decline and exit')
        .width('100%').enabled(!this.busy)
        .onClick(() => { (getContext(this) as common.UIAbilityContext).terminateSelf(); })
    }.padding(24).width('100%').height('100%')
  }
}
