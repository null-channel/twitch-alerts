// Main component for displaying alerts
import useWebSocket from "react-use-websocket";
import { useQueueState } from "rooks";
import { useState, useEffect, useRef, useCallback } from "react";

const WS_URL = "ws://127.0.0.1:9000";

function DisplayAlert() {
  const [message, setMessage] = useState("");
  const [list, { enqueue, dequeue }] = useQueueState([""]);

  const addToStack = useCallback(
    (event: any) => {
      const data = event.data;
      enqueue(data);
    },
    [enqueue]
  );

  useWebSocket(WS_URL, {
    onOpen: () => {
      console.log("WebSocket connection established.");
    },
    share: true,
    shouldReconnect: (closeEvent) => true,
    retryOnError: true,
    onError: (event) => console.log(event),
    onClose: (event) => console.log(event),
    onMessage: addToStack,
  });
  let [timer, setTimer] = useState<NodeJS.Timeout>();
  let timerRef = useRef(timer);
  let setTimerRef = useRef(setTimer);

  const endTimer = useCallback(() => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      dequeue();
      setTimerRef.current(setTimeout(() => {
        endTimer();
      }, 5000));
    } else {
      setTimerRef.current(setTimeout(() => {
        endTimer();
      }, 5000));
    }

  }, [timerRef, setTimer, dequeue]);

  useEffect(() => {
    const timer = setTimeout(() => {
      if (list.length > 0) {
        dequeue();
      }
    }, 5000);
    return () => clearTimeout(timer);
  }, [list, dequeue]);

  useEffect(() => {
    if (list.length > 0) {
      console.log("list is not empty");
      console.log(list[0]);
      console.log(list);
      
      setMessage(list[0]);
    } else {
      setMessage("");
    }
  }, [list]);
  return (
    <div className="App">
      <header className="App-header">
        <p> {message} </p>
      </header>
    </div>
  );
}

export default DisplayAlert;