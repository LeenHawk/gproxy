import { common } from '@kit.AbilityKit';
import { emitter } from '@kit.BasicServicesKit';
import { statusBarManager } from '@kit.DeskTopExtensionKit';
import { image } from '@kit.ImageKit';
import native from 'libgproxy_host_tauri.so';

/** PC status bar. The mobile UI never offers this capability. */
export class GproxyTray {
  public ready: boolean = false;
  private icon: image.PixelMap | undefined;
  private onClick = (_event: emitter.EventData): void => { this.open(); };
  private onMenu = (event: emitter.EventData): void => {
    if (event.data?.['menuCode'] === 'quit') {
      this.context.terminateSelf().catch((error: Error) => console.error(error.message));
    } else { this.open(); }
  };

  private context: common.UIAbilityContext;

  constructor(context: common.UIAbilityContext) { this.context = context; }

  open(): void {
    this.context.startAbility({ bundleName: this.context.applicationInfo.name,
      abilityName: this.context.abilityInfo.name }).catch((error: Error) => console.error(error.message));
  }

  private menus(): statusBarManager.StatusBarGroupMenu[] {
    const language = native.gproxyLanguage();
    const open = language === 'zh-CN' ? '打开 GPROXY' : language === 'zh-TW' ? '開啟 GPROXY' : 'Open GPROXY';
    const quit = language === 'zh-CN' ? '退出 GPROXY' : language === 'zh-TW' ? '結束 GPROXY' : 'Quit GPROXY';
    const active = native.gproxyEngineRunning();
    const status = language === 'zh-CN' ? (active ? '代理正在运行' : '代理已停止')
      : language === 'zh-TW' ? (active ? '代理正在執行' : '代理已停止')
      : (active ? 'Proxy is running' : 'Proxy has stopped');
    return [[
      { title: status, menuAction: { abilityName: this.context.abilityInfo.name } },
      { title: open, menuAction: { abilityName: this.context.abilityInfo.name } },
      { title: quit, menuAction: { abilityName: this.context.abilityInfo.name, notifyOnly: true, menuCode: 'quit' } },
    ]];
  }

  async install(): Promise<void> {
    if (this.ready) {
      statusBarManager.updateStatusBarMenu(this.context, this.menus());
      return;
    }
    const source = image.createImageSource(this.context.resourceManager.getRawFileContentSync('gproxy-tray.png').buffer);
    try {
      this.icon = await source.createPixelMap({ desiredSize: { width: 24, height: 24 } });
      statusBarManager.on('statusBarIconClick', this.onClick);
      statusBarManager.on('rightMenuClick', this.onMenu);
      statusBarManager.addToStatusBar(this.context, {
        icons: { white: this.icon, black: this.icon },
        quickOperation: { title: 'GPROXY', height: 1, abilityName: '' },
        statusBarGroupMenu: this.menus(),
      });
      this.ready = true;
    } catch (error) {
      await this.remove();
      throw error;
    } finally {
      await source.release();
    }
  }

  async hide(): Promise<void> {
    if (!this.ready) throw new Error('The status bar icon is not available');
    await this.context.hideAbility();
  }

  async remove(): Promise<void> {
    if (this.ready) statusBarManager.removeFromStatusBar(this.context);
    statusBarManager.off('statusBarIconClick', this.onClick);
    statusBarManager.off('rightMenuClick', this.onMenu);
    this.ready = false;
    await this.icon?.release();
    this.icon = undefined;
  }
}
