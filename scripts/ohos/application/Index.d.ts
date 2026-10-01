export const registerGproxyBackground: (handler: (command: string) => Promise<string>) => void;
export const gproxyEngineRunning: () => boolean;
export const gproxyShutdown: () => void;
export const gproxyAutoStartupStatus: () => Promise<boolean>;
export const gproxyLanguage: () => string;
