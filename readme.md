# reload

Watches for file changes and runs commands in response.

## Example

The following example is based on this config:

```yaml
version: "0"
paths:
  - path: "."
    pattern: "**/example.{txt,log}"
    command: ["cat", "example.txt", "example.log"]
```

Then use reload to execute the command when file changes happen:

```text
echo "test 1" > example.txt
./reload
# test 1
echo "test 2" > example.log
# test 1
# test 2
rm example.txt
# test 2
```

## Future

This is an extremely early release. Expect breaking changes, even for minor or patch version increments until 1.0.0.
