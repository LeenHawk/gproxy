import { abilityAccessCtrl, common, wantAgent } from '@kit.AbilityKit';
import { backgroundTaskManager } from '@kit.BackgroundTasksKit';
import { preferences } from '@kit.ArkData';
import { BusinessError, deviceInfo } from '@kit.BasicServicesKit';
import native from 'libgproxy_host_tauri.so';
import type { GproxyTray } from './GproxyTray';
import window from '@ohos.window';

interface BackgroundStatus {
  supported: boolean;
  deviceType: string;
  enabled: boolean;
  active: boolean;
  cancelled: boolean;
  error: string | null;
  autoStart: boolean | null;
  startupError: string | null;
  startMinimized: boolean;
  canMinimize: boolean;
  canTray: boolean;
  tray: boolean;
  trayReady: boolean;
  closeToTray: boolean;
  trayError: string | null;
}

export class GproxyBackground {
  private storage: preferences.Preferences | undefined;
  private enabled: boolean = false;
  private active: boolean = false;
  private cancelled: boolean = false;
  private error: string | null = null;
  private autoStart: boolean | null = null;
  private startupError: string | null = null;
  private startMinimized: boolean = true;
  private trayEnabled: boolean = true;
  private closeToTray: boolean = true;
  private trayError: string | null = null;
  private tray: GproxyTray | undefined;
  private mainWindow: window.Window | undefined;
  private queue: Promise<void> = Promise.resolve();
  private observing: boolean = false;
  private destroyed: boolean = false;
  private onCancel = (_info: backgroundTaskManager.ContinuousTaskCancelInfo): void => {
    // Removing the system notification is an explicit stop, not a retry signal.
    this.active = false;
    this.enabled = false;
    this.cancelled = true;
    this.dispatch('cancel').catch((error: Error) => { this.error = error.message; });
  };

  private context: common.UIAbilityContext;
  private autoStartup: boolean;

  constructor(context: common.UIAbilityContext, autoStartup: boolean) {
    this.context = context;
    this.autoStartup = autoStartup;
  }

  dispatch(command: string): Promise<string> {
    const result = this.queue.then(() => this.execute(command));
    this.queue = result.then(() => {}, () => {});
    return result;
  }

  async foreground(): Promise<void> {
    try { await this.dispatch('restore'); }
    catch (error) { this.error = (error as Error).message; }
  }

  private async supported(): Promise<boolean> {
    if (deviceInfo.deviceType === '2in1' || deviceInfo.deviceType === 'pc') return true;
    // API 21 extends taskKeeping to phones/tablets only with an approved ACL.
    // An ordinary API 20 HAP cannot acquire that permission by asking the user.
    if (deviceInfo.sdkApiVersion < 21) return false;
    const granted = await abilityAccessCtrl.createAtManager().checkAccessToken(
      this.context.applicationInfo.accessTokenId, 'ohos.permission.KEEP_BACKGROUND_RUNNING_SYSTEM');
    return granted === abilityAccessCtrl.GrantStatus.PERMISSION_GRANTED;
  }

  private async execute(command: string): Promise<string> {
    if (this.destroyed) throw new Error('The application is closing');
    const desktop = deviceInfo.deviceType === '2in1' || deviceInfo.deviceType === 'pc';
    if (!this.storage) {
      this.storage = await preferences.getPreferences(this.context, { name: 'gproxy_background' });
      this.enabled = await this.storage.get('enabled', desktop) as boolean;
      this.startMinimized = await this.storage.get('startMinimized', true) as boolean;
      this.trayEnabled = await this.storage.get('tray', true) as boolean;
      this.closeToTray = await this.storage.get('closeToTray', true) as boolean;
    }
    const supported = await this.supported();
    const canTray = desktop && canIUse('SystemCapability.PCService.StatusBarManager');
    if (canTray && (command === 'restore' || command === 'refresh-tray' || command === 'tray-on')) {
      if ((this.trayEnabled || command === 'tray-on') && (native.gproxyEngineRunning() || this.tray?.ready)) {
        try {
          if (!this.tray) {
            const statusBar = await import('./GproxyTray');
            this.tray = new statusBar.GproxyTray(this.context);
          }
          await this.tray.install(); this.trayError = null;
        }
        catch (error) { this.trayError = (error as Error).message; if (command === 'tray-on') throw error; }
      }
    }
    if (command === 'tray-on' || command === 'tray-off') {
      if (!canTray) throw new Error('The status bar is not available on this device');
      const enabled = command === 'tray-on';
      if (!enabled) { this.tray?.open(); await this.tray?.remove(); }
      await this.storage.put('tray', enabled);
      if (!enabled) { await this.storage.put('closeToTray', false); this.closeToTray = false; }
      await this.storage.flush();
      this.trayEnabled = enabled;
    }
    if (command === 'close-hide' || command === 'close-quit') {
      const enabled = command === 'close-hide';
      if (enabled && (!this.tray?.ready || !this.active)) throw new Error('Enable the status bar and continuous background running first');
      await this.storage.put('closeToTray', enabled);
      await this.storage.flush();
      this.closeToTray = enabled;
    }
    try {
      this.autoStart = await native.gproxyAutoStartupStatus();
      this.startupError = null;
    } catch (error) {
      this.autoStart = null;
      this.startupError = (error as Error).message;
    }
    if (command === 'startup-settings') {
      await this.context.startAbility({
        bundleName: 'com.huawei.hmos.settings',
        abilityName: 'com.huawei.hmos.settings.MainAbility',
        uri: 'pc_app_setup_settings',
      });
    }
    if (command === 'minimize-on' || command === 'minimize-off') {
      const enabled = command === 'minimize-on';
      await this.storage.put('startMinimized', enabled);
      await this.storage.flush();
      this.startMinimized = enabled;
    }
    try {
      if (supported && !this.observing) {
        backgroundTaskManager.on('continuousTaskCancel', this.onCancel);
        this.observing = true;
      }
      if (supported) {
        const tasks = await backgroundTaskManager.getAllContinuousTasks(this.context);
        this.active = tasks.some(task => task.abilityName === this.context.abilityInfo.name);
      }
      if (command === 'enable' || (command === 'restore' && this.enabled && !this.cancelled && native.gproxyEngineRunning())) {
        if (!supported) throw new Error('Continuous background running is not available on this device');
        if (!native.gproxyEngineRunning()) throw new Error('Start the proxy before enabling background running');
        if (!this.active) {
          const agent = await wantAgent.getWantAgent({
            wants: [{ bundleName: this.context.applicationInfo.name, abilityName: this.context.abilityInfo.name }],
            actionType: wantAgent.OperationType.START_ABILITY,
            requestCode: 0,
            actionFlags: [wantAgent.WantAgentFlags.UPDATE_PRESENT_FLAG],
          });
          await backgroundTaskManager.startBackgroundRunning(this.context, backgroundTaskManager.BackgroundMode.TASK_KEEPING, agent);
          this.active = true;
        }
        await this.storage.put('enabled', true);
        await this.storage.flush();
        this.enabled = true;
        this.cancelled = false;
        this.error = null;
      } else if (command === 'disable' || command === 'cancel') {
        if (command === 'disable' && this.active) {
          await backgroundTaskManager.stopBackgroundRunning(this.context);
        }
        await this.storage.put('enabled', false);
        await this.storage.flush();
        this.enabled = false;
        this.active = false;
        this.error = null;
      }
    } catch (error) {
      const failure = error as BusinessError;
      this.error = failure.code ? `${failure.code}: ${failure.message}` : failure.message;
      if (command !== 'status' && command !== 'restore') throw new Error(this.error);
    }
    if (command === 'restore' && desktop && this.autoStartup && this.startMinimized && this.active && native.gproxyEngineRunning() && this.mainWindow) {
      this.autoStartup = false;
      try {
        if (this.tray?.ready) await this.tray?.hide();
        else if (!this.trayEnabled) await this.mainWindow.minimize();
        // A failed tray installation leaves the window visible with an error.
      } catch (error) { this.trayError = (error as Error).message; }
    }
    const status: BackgroundStatus = { supported, deviceType: deviceInfo.deviceType,
      enabled: this.enabled, active: this.active, cancelled: this.cancelled, error: this.error, autoStart: this.autoStart, startupError: this.startupError,
      startMinimized: this.startMinimized, canMinimize: desktop, canTray, tray: canTray && this.trayEnabled, trayReady: this.tray?.ready ?? false,
      closeToTray: canTray && this.trayEnabled && this.closeToTray && this.active, trayError: this.trayError };
    return JSON.stringify(status);
  }

  attachWindow(mainWindow: window.Window): void {
    this.mainWindow = mainWindow;
    if (deviceInfo.deviceType === '2in1' || deviceInfo.deviceType === 'pc') {
      try {
        mainWindow.on('windowWillClose', async () => {
          try { return await this.closeInBackground(); }
          catch (error) { this.trayError = (error as Error).message; return true; }
        });
      } catch (error) { this.trayError = (error as Error).message; }
    }
  }

  manualOpen(): void { this.autoStartup = false; }

  async closeInBackground(): Promise<boolean> {
    if (!this.closeToTray || !this.tray?.ready || !this.active) return false;
    await this.tray?.hide();
    return true;
  }

  async destroy(): Promise<void> {
    this.destroyed = true;
    await this.queue;
    if (this.observing) backgroundTaskManager.off('continuousTaskCancel', this.onCancel);
    if (this.active) await backgroundTaskManager.stopBackgroundRunning(this.context);
    if (this.tray?.ready) await this.tray?.remove();
  }
}
