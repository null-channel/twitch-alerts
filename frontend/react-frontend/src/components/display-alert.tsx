// Main component for displaying alerts
import useWebSocket from 'react-use-websocket';
import { useQueueState } from "rooks";
import { useState, useEffect, useRef } from 'react';


const WS_URL = 'ws://127.0.0.1:9000';

function DisplayAlert() {
  useWebSocket(WS_URL, {
    onOpen: () => {
      console.log('WebSocket connection established.');
    },
    share: true,
    shouldReconnect: (closeEvent) => true,
    retryOnError: true,
    onError: (event) => console.log(event),
    onClose: (event) => console.log(event),
    onMessage: (event) => addToStack(event),
  });

  const [message, setMessage] = useState("");
  const [list, { enqueue, dequeue }] = useQueueState(["1","2","3"]);

  function addToStack(event: WebSocketEventMap['message']) {
    enqueue(event.data);
  }

  useEffect(() => {
    const interval = setInterval(() => {
      timerEndChecks();
    }, 500);
    return () => clearInterval(interval);
  }, [dequeue])


  function timerEndChecks() {
    console.log("timer ended");
    setMessage("");
    checkQueue();
  }

  function checkQueue() {
    console.log("checking queue: " + list);
    //const message = dequeueRef.current();
    const message = dequeue();
    console.log("message: "+ message);
    if (message != undefined) {
      setMessage(message);
    }
    console.log("list: "+ list);
  }

  const message_queue = [];
  return (
    <div className="App">
      <header className="App-header">
        <p> {message} </p>
      </header>
    </div>
  );
}

export default DisplayAlert;