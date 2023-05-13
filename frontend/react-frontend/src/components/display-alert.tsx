// Main component for displaying alerts
import useWebSocket from 'react-use-websocket';
import { useQueueState } from "rooks";
import { useState, useEffect, useRef } from 'react';


const WS_URL = 'ws://127.0.0.1:9000';

function DisplayAlert() {
  const { sendJsonMessage, readyState } = useWebSocket(WS_URL, {
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
  const [list, { enqueue, dequeue }] = useQueueState([{value:"1",timeSeconds:5},{value:"2",timeSeconds:5},{value:"3",timeSeconds:5}]);
  const [displayingMessage, setDispalyingMessage] = useState<boolean|undefined>();
  const [doneTime, setDoneTime] = useState<Date|undefined>()
  const [running, setRunning] = useState(true);
  if (displayingMessage === undefined) {
    setDispalyingMessage(false);
  }

  function addToStack(event: WebSocketEventMap['message']) {
    console.log("new ws message");
    sendJsonMessage({message:"got your message sir."})
    enqueue({value:event.data,timeSeconds:5});
  }

  const handle = useEffect(() => {
      const interval = setInterval(() => {
        timerEndChecks();
      }, 1000);
      return () => clearInterval(interval);
  }, [dequeue,sendJsonMessage])

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
      setMessage(message.value);
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