curl  -vvv -u "admin:pass1234" --cert server.pem --key server.key -k --data-binary @dns.json -H "Content-Type: application/json" -X PUT https://192.168.44.113:48888/apiserver/config/dns
