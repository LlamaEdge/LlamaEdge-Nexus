# test /v1/chat/completions endpoint
POST http://localhost:10086/v1/chat/completions
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
    "model": "Qwen2.5-0.5B-Instruct",
    "stream": false
}
```
HTTP 200
[Asserts]
jsonpath "$.model" == "Qwen2.5-0.5B-Instruct"
jsonpath "$.choices[0].message.content" contains "Paris"
