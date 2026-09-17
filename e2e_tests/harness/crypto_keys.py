"""
Cryptographic keys and signature helpers for DANA SNAP Open API testing.
Provides test RSA-2048 key pairs for signing and verifying test webhooks.
"""

import base64
from typing import Optional

try:
    from cryptography.hazmat.primitives import hashes, serialization
    from cryptography.hazmat.primitives.asymmetric import padding
    HAS_CRYPTO = True
except ImportError:
    HAS_CRYPTO = False

DANA_TEST_PUBLIC_KEY_PEM = """-----BEGIN PUBLIC KEY-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAlG6urqDDVNHbTJew+/Ja
i/5Fk4XG33TeAykJr7JUk9buQ5pQS4J6SfCxbWilC6b8LzHUVUbvQoszYg6FoN9+
ovVbdZK1tLkU0nrz8RaUuQQfnrQNoaXRi+/G5BmhYsDCB46bTY9r1lfQB+4P3Gha
Rj1qVyJTK6y56XhERLtoa1ho5QmHKRRj8gbkEw5jaILnZikB8elS/8xLYUUIah0n
B0JhARJ5U5muNg2CrKoGE4jV7TqCQmrV+q74wGEFoiLnT347EKsu26Ns7wJn3TDV
u0qpjn+HvG+77A9kT1/88b+DdakpfYtpq8ENwKxCod8THAEMa386ZKncnxJp96U7
RwIDAQAB
-----END PUBLIC KEY-----"""

DANA_TEST_PRIVATE_KEY_PEM = """-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQCUbq6uoMNU0dtM
l7D78lqL/kWThcbfdN4DKQmvslST1u5DmlBLgnpJ8LFtaKULpvwvMdRVRu9CizNi
DoWg336i9Vt1krW0uRTSevPxFpS5BB+etA2hpdGL78bkGaFiwMIHjptNj2vWV9AH
7g/caFpGPWpXIlMrrLnpeEREu2hrWGjlCYcpFGPyBuQTDmNogudmKQHx6VL/zEth
RQhqHScHQmEBEnlTma42DYKsqgYTiNXtOoJCatX6rvjAYQWiIudPfjsQqy7bo2zv
AmfdMNW7SqmOf4e8b7vsD2RPX/zxv4N1qSl9i2mrwQ3ArEKh3xMcAQxrfzpkqdyf
Emn3pTtHAgMBAAECggEAA5JaP7d8m8jk9wXba2SciyvWLsOUUoI0aW0OX5zx7hDI
8PWAoyCDos3Y5yISfqJJBTW0v0ySq05AMUbaLlHScUdoKP8bwjqF5r6wqgd6Eq2n
uSDqBw6/aRee+JQpTwAGazoiQI6H8MNyLQ6scQhNy8zkhy47RBzG6HhNZD4CODsB
/84BY1Xxz+7U0E00yh3T1Y9LRHbA+c1+Psy219liXRj0Vo4zC5nCY3sBaSXZbAYL
sVFF12c3oHp6K0ITMSFeOUtJbYdpf072zMb6VzngwNORGgEUWxZjklXJOlExhk9d
8dtaS382v531o3r2lHNVG2h9nywYDL8oF4SG+JgNIQKBgQDKdq1z6+SVom0cqO50
+AFSHTE2nOl+H2ahxF/G5Xm9hMnBZMInXa3i0VdYdZb0wsHNA46CoIsttfcYF+Xg
94DG6bwK7cNGCFyLc3gYDsh2Iv7aOqrgrEnvauo4hPmsQnQJVelHbe4/asmeDf7i
1IVSMvRO/AW6mdTQN29qEsLfMQKBgQC7rnsMsLbX6JOmw5E5tbjyZtEGTGRoL2Cf
VpyWptqSLVtVLdb1oBQOEtTdFPE1sunR/C7510WZDvgOeqnSfhAhlrNMaYwGQMbx
wgdzph0N06iDllun2P7k5jiPSMoDfpzDyYCMX4/QWTOe3ZkSoGYAGacdRFcMZibk
CWH8oLNT9wKBgEr+8vlBpAaZh/lZyhqh0ztrfNNSBFunngjGCQRP9GxzR5jPjeuv
E7409TnbNPOtQMSEUMGqXmOsR78w+wH+LEGCSxlxQSgr6LvvJckjkLXR+L01hh57
M1fwLpqJB0L7yqe6nxLKcbokAFL/tC6pskjkfwLS7/xTBzWpkyejk3PBAoGAPE+B
cz6GQzOV3w0Raf4fhKXNnbyGt4QiBJIMl8zeiALTSrgET8I1L6CVjsXgDWWFBdmI
LvkigGDzDZQVZnLkNCb9TxzLxmaih6XWRy+mPn85s69pnLJ6lov0uPanFCBnt/LU
wEclK8q+b9q+CeJJZNbZgOopHu7kqHrrZgcuGVkCgYEApPDkY+y94L2SkgNGbd64
yenZFpNidcr/xLvCVrTg12EmFJ/nYsjHc2/r9W60X11NREi7rUCJhN34i3c8YdjM
sZ+axdF1bmPmcqNVO4YFgORFdAsi1XDsqWCbl82zIH59mqEcuaLk2gS3WjIB5KZ8
4StTj0O1GAI65ENXcr4cWR4=
-----END PRIVATE KEY-----"""

def sign_dana_payload(string_to_sign: str) -> str:
    """Sign string with RSA-SHA256 private key and return base64 encoded signature."""
    if not HAS_CRYPTO:
        return "SIMULATED_RSA_SHA256_SIGNATURE"
    try:
        priv_key = serialization.load_pem_private_key(
            DANA_TEST_PRIVATE_KEY_PEM.encode("utf-8"),
            password=None
        )
        sig = priv_key.sign(
            string_to_sign.encode("utf-8"),
            padding.PKCS1v15(),
            hashes.SHA256()
        )
        return base64.b64encode(sig).decode("utf-8")
    except Exception as e:
        return f"ERR_SIGNING_{e}"

def verify_dana_signature(string_to_sign: str, sig_b64: str) -> bool:
    """Verify base64 encoded RSA-SHA256 signature against DANA public key."""
    if not HAS_CRYPTO:
        return sig_b64 == "SIMULATED_RSA_SHA256_SIGNATURE"
    try:
        pub_key = serialization.load_pem_public_key(
            DANA_TEST_PUBLIC_KEY_PEM.encode("utf-8")
        )
        sig_bytes = base64.b64decode(sig_b64)
        pub_key.verify(
            sig_bytes,
            string_to_sign.encode("utf-8"),
            padding.PKCS1v15(),
            hashes.SHA256()
        )
        return True
    except Exception:
        return False
