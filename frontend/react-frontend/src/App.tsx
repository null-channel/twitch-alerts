import './App.css';
import DisplayAlert from './components/display-alert';
import useWebSocket from 'react-use-websocket';

const WS_URL = 'ws://127.0.0.1:9000';

function App() {  
  /*
  useWebSocket(WS_URL, {
    onOpen: () => {
      console.log('WebSocket connection established.');
    },
    shouldReconnect: (closeEvent) => true,
    retryOnError: true,
    onError: (event) => console.log(event),
    onClose: (event) => console.log(event),
  });
  */
  return (
    <div className="App">
      <header className="App-header">
        <DisplayAlert></DisplayAlert>
      </header>
    </div>
  );
}

export default App;
