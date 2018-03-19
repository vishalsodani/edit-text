import * as commands from './commands';
import * as util from './util';
import * as interop from './interop';

export interface Network {
  onNativeMessage: (any) => void;
  onNativeClose: () => void;
  onSyncClose: () => void;

  nativeConnect(): Promise<void>;
  nativeCommand(command: commands.Command): void;
  syncConnect(): Promise<void>;
}

export class ProxyNetwork implements Network {
  editorID: string;

  nativeSocket: WebSocket;

  onNativeMessage: (any) => void;
  onNativeClose: () => void;
  onSyncClose: () => void; // unused

  nativeCommand(command: commands.Command) {
    this.nativeSocket.send(JSON.stringify(command));
  }

  nativeConnect(): Promise<void> {
    let network = this;
    return Promise.resolve()
    .then(() => {
      this.nativeSocket = new WebSocket(
        util.clientProxyUrl() + 
        (window.location.hash == '#helloworld' ? '?helloworld' : '')
      );
      this.nativeSocket.onopen = function (event) {
        console.log('Editor "%s" is connected.', network.editorID);
      };
      this.nativeSocket.onmessage = function (event) {
        let parse = JSON.parse(event.data);
        network.onNativeMessage(parse);
      };
      this.nativeSocket.onclose = network.onNativeClose;
    });
  }

  // The native server (the client proxy) handles sync traffic directly
  syncConnect(): Promise<void> {
    return Promise.resolve();
  }
}


export class WasmNetwork implements Network {
  editorID: string;

  nativeSocket: WebSocket;
  syncSocket: WebSocket;

  // TODO remove this
  Module: any;

  onNativeMessage: (any) => void;
  onNativeClose: () => void; // unused
  onSyncClose: () => void;

  nativeCommand(command: commands.Command) {
    this.Module.wasm_command({
      NativeCommand: command,
    });
  }

  // Wasm connector.
  nativeConnect(): Promise<void> {
    const network = this;
    return new Promise((reject, resolve) => {
      interop.instantiate(function (data) {
        // console.log('----> js_command:', data);

        // Make this async so we don't have deeply nested call stacks from Rust<->JS interop.
        setImmediate(() => {
          // Parse the packet.
          let parse = JSON.parse(data);

          if (parse.SyncServerCommand) {
            network.syncSocket.send(JSON.stringify(parse.SyncServerCommand));
          } else {
            network.onNativeMessage(parse);
          }
        });
      })
      .then(Module => {
        Module.wasm_setup();

        setImmediate(() => {
          // Websocket port
          network.Module = Module;
          resolve();
        });
      })
    });
  }

  syncConnect(): Promise<void> {
    let network = this;

    return Promise.resolve()
    .then(() => {
      let syncSocket = new WebSocket(
        util.syncUrl() + (window.location.hash == '#helloworld' ? '?helloworld' : '')
      );
      syncSocket.onopen = function (event) {
        console.log('Editor "%s" is connected.', network.editorID);
      };

      // Keepalive
      setInterval(() => {
        syncSocket.send(JSON.stringify({
          Keepalive: null,
        }));
      }, 1000);

      syncSocket.onmessage = function (event) {
        // console.log('Got message from sync:', event.data);
        network.Module.wasm_command({
          SyncClientCommand: JSON.parse(event.data),
        });
      };

      syncSocket.onclose = network.onSyncClose;
    });
  }
}