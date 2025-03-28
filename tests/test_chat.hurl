
# test /v1/chat/completions endpoint
POST http://localhost:9068/v1/chat/completions
Accept: application/json
Content-Type: application/json
```json
{
    "messages": [
        {
            "role": "user",
            "content": "What is the capital of France?"
        }
    ],
    "model": "Llama-3.2-1B",
    "stream": false
}
```
HTTP 200
[Asserts]
jsonpath "$.model" == "Llama-3.2-1B"
jsonpath "$.choices[0].message.content" contains "Paris"
