# AWS Lambda Extension for log-store


# Install

1. Create your AWS Lambda function
2. Build this extension: `cargo lambda build --release --extension`
3. Deploy the extension: `cargo lambda deploy --extension`
   4. You might have to run `aws credentials` first
1. Add a layer to your Lambda function
2. Select the log-store extension, and the most recent version
3. Set the environment variable `LOG_STORE_ADDRESS` to your log-store instance's IP and port (usually 1234)

Used to "publish" the extension:

```
aws lambda add-layer-version-permission --layer-name lambda_extension --version-number 18 --statement-id allOrgs --principal '*' --region 'us-east-1' --action lambda:GetLayerVersion
```

