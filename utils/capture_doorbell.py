import io
import cv2
import pushover_complete

import paho.mqtt.client as mqtt


def send_notification():
    url = "rtsp://reolink:reolink0@192.168.80.102:554"
    user_key = "uomfetdtawqotwp3ii9jpf4buys3p4"
    api_token = "amfa9dzeck8bongtab3nrta3xux3hj"

    cap = cv2.VideoCapture(url)
    ret, frame = cap.read()
    cap.release()

    encode_param = [int(cv2.IMWRITE_JPEG_QUALITY), 9]
    res, img = cv2.imencode('.jpg', frame, encode_param)

    f = io.BytesIO(img.tobytes())

    client = pushover_complete.PushoverAPI(api_token)
    client.send_message(user_key, "Doorbell", title="Doorbell (Image)", image=f)

def on_connect(client, userdata, flags, reason_code, properties):
    print(f"MQTT Connected: {reason_code}")
    client.subscribe("doorbell/ring")

def on_message(client, userdata, msg):
    if msg.topic == "doorbell/ring" and msg.payload == b"ON":
        print(f"{msg.topic}: {msg.payload}")
        send_notification()

mqttc = mqtt.Client(mqtt.CallbackAPIVersion.VERSION2)
mqttc.on_connect = on_connect
mqttc.on_message = on_message

mqttc.connect("192.168.60.1", 1883, 60)
mqttc.loop_forever()
