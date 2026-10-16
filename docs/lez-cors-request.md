**Allow browser access (CORS) on the LEZ testnet RPC**

`https://testnet.lez.logos.co` doesn't allow cross-origin requests: an `OPTIONS` preflight returns 405, and responses carry no `Access-Control-Allow-Origin`. A web page therefore can't read the chain directly.

That pushes browser apps toward a relay or proxy, which sees every visitor's IP and which accounts they read. That's a bad fit for privacy apps on LEZ. LNS (the `.lez` name service), for example, wants a static page that resolves a name entirely in the visitor's browser (spec: https://github.com/jefdiesel/logos-names/blob/main/SPEC.md, section 12.3). It can't do that without CORS on the sequencer.

Request: have the testnet RPC answer the preflight and send
```
Access-Control-Allow-Origin: *
Access-Control-Allow-Methods: POST, OPTIONS
Access-Control-Allow-Headers: content-type
```
The RPC is already public and unauthenticated, so `*` exposes nothing new. It only lets pages do what curl can already do.

To check:
```
curl -si -X OPTIONS https://testnet.lez.logos.co -H 'Origin: https://example.com' -H 'Access-Control-Request-Method: POST'
```
