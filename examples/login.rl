# rate limits for the auth service
rule login {
    rate = 5/sec
    burst = 10
}

rule password_reset {
    rate = 3/min
    burst = 1
}

rule bulk_export {
    rate = 100/hour
    burst = 20
}
