// The few Chrome extension APIs this extension uses, declared by hand (no @types/chrome,
// per specs/dependencies.md). Shapes follow https://developer.chrome.com/docs/extensions/reference/api.

declare namespace chrome {
  namespace runtime {
    const id: string;
    const lastError: { message?: string } | undefined;
    interface MessageSender {
      id?: string;
      tab?: { id?: number; url?: string };
      url?: string;
    }
    function sendMessage<T = unknown>(message: unknown): Promise<T>;
    function sendNativeMessage(application: string, message: object): Promise<unknown>;
    function getURL(path: string): string;
    function getManifest(): { version: string };
    const onMessage: {
      addListener(
        cb: (message: unknown, sender: MessageSender, sendResponse: (response: unknown) => void) => boolean | void,
      ): void;
    };
    const onInstalled: { addListener(cb: () => void): void };
    const onStartup: { addListener(cb: () => void): void };
  }

  namespace i18n {
    function getMessage(name: string, substitutions?: string | string[]): string;
  }

  namespace storage {
    interface Area {
      get(keys: string | string[]): Promise<Record<string, unknown>>;
      set(items: Record<string, unknown>): Promise<void>;
    }
    const local: Area;
    const onChanged: { addListener(cb: (changes: Record<string, { newValue?: unknown }>, area: string) => void): void };
  }

  namespace permissions {
    interface Permissions {
      permissions?: string[];
      origins?: string[];
    }
    function contains(p: Permissions): Promise<boolean>;
    function request(p: Permissions): Promise<boolean>;
    function remove(p: Permissions): Promise<boolean>;
    const onAdded: { addListener(cb: (p: Permissions) => void): void };
    const onRemoved: { addListener(cb: (p: Permissions) => void): void };
  }

  namespace scripting {
    interface RegisteredContentScript {
      id: string;
      matches?: string[];
      js?: string[];
      runAt?: "document_start" | "document_end" | "document_idle";
      persistAcrossSessions?: boolean;
    }
    function registerContentScripts(scripts: RegisteredContentScript[]): Promise<void>;
    function unregisterContentScripts(filter?: { ids?: string[] }): Promise<void>;
    function getRegisteredContentScripts(filter?: { ids?: string[] }): Promise<RegisteredContentScript[]>;
    function executeScript(injection: { target: { tabId: number }; files: string[] }): Promise<unknown>;
  }

  namespace tabs {
    interface Tab {
      id?: number;
      url?: string;
    }
    function query(q: { active?: boolean; currentWindow?: boolean }): Promise<Tab[]>;
    function create(p: { url: string }): Promise<Tab>;
  }

  namespace contextMenus {
    interface OnClickData {
      menuItemId: string | number;
      linkUrl?: string;
    }
    function create(p: { id: string; title: string; contexts: string[]; targetUrlPatterns?: string[] }): void;
    function removeAll(): Promise<void>;
    const onClicked: { addListener(cb: (info: OnClickData) => void): void };
  }

  namespace cookies {
    interface Cookie {
      name: string;
      value: string;
    }
    function getAll(details: { url: string }): Promise<Cookie[]>;
  }

  namespace downloads {
    interface DownloadItem {
      id: number;
      url: string;
      finalUrl: string;
      state: "in_progress" | "interrupted" | "complete";
      byExtensionId?: string;
      incognito: boolean;
      mime: string;
      /** Chrome's safety verdict: "safe", "accepted" (the user kept it), or a warning or a
       * scan still running ("file", "url", "content", "uncommon", "host", "unwanted",
       * "asyncScanning", ...). */
      danger: string;
      /** The page the download was started from, when Chrome knows it. */
      referrer?: string;
      filename?: string;
    }
    function search(q: { id: number }): Promise<DownloadItem[]>;
    function pause(id: number): Promise<void>;
    function resume(id: number): Promise<void>;
    function cancel(id: number): Promise<void>;
    function erase(q: { id: number }): Promise<number[]>;
    const onCreated: { addListener(cb: (item: DownloadItem) => void): void };
  }
}
