import { render } from 'preact';
import { useEffect, useState } from 'preact/hooks';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

function App() {
  const [state, setState] = useState<unknown>(null);
  useEffect(() => {
    const refresh = () => invoke('get_state').then(setState);
    refresh();
    const un = listen('status', refresh);
    return () => {
      un.then((f) => f());
    };
  }, []);
  return <pre>{JSON.stringify(state, null, 2)}</pre>;
}

render(<App />, document.getElementById('app')!);
