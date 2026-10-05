#!/usr/bin/env python3
"""Generates the TLS fixtures of the runner's transport tests.

Test material only. The keys exist so that a loopback test server can
present certificates; they protect nothing and are not secrets.

Writes, next to this script, DER files:
  ca.der                       test CA (trusted by the tests)
  localhost.der, .key.der      server certificate for "localhost" from the CA
  other_name.der, .key.der     server certificate for "other.test" from the CA
  other_ca.der                 a second CA the tests do not trust

ECDSA P-256 with SHA-256 throughout. Valid from 2024-01-01 to 2049-12-31,
so that a machine with a slightly wrong clock still accepts them.
Requires the Python package `cryptography`.
"""
import datetime
import os

from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec
from cryptography.x509.oid import ExtendedKeyUsageOID, NameOID

HERE = os.path.dirname(os.path.abspath(__file__))
NOT_BEFORE = datetime.datetime(2024, 1, 1, tzinfo=datetime.timezone.utc)
NOT_AFTER = datetime.datetime(2049, 12, 31, tzinfo=datetime.timezone.utc)


def name(common_name):
    return x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, common_name)])


def ca(common_name):
    key = ec.generate_private_key(ec.SECP256R1())
    cert = (
        x509.CertificateBuilder()
        .subject_name(name(common_name))
        .issuer_name(name(common_name))
        .public_key(key.public_key())
        .serial_number(x509.random_serial_number())
        .not_valid_before(NOT_BEFORE)
        .not_valid_after(NOT_AFTER)
        .add_extension(x509.BasicConstraints(ca=True, path_length=0), critical=True)
        .add_extension(
            x509.KeyUsage(
                digital_signature=False, content_commitment=False, key_encipherment=False,
                data_encipherment=False, key_agreement=False, key_cert_sign=True,
                crl_sign=True, encipher_only=False, decipher_only=False,
            ),
            critical=True,
        )
        .add_extension(x509.SubjectKeyIdentifier.from_public_key(key.public_key()), critical=False)
        .sign(key, hashes.SHA256())
    )
    return key, cert


def server(dns_name, issuer_key, issuer_cert):
    key = ec.generate_private_key(ec.SECP256R1())
    cert = (
        x509.CertificateBuilder()
        .subject_name(name(dns_name))
        .issuer_name(issuer_cert.subject)
        .public_key(key.public_key())
        .serial_number(x509.random_serial_number())
        .not_valid_before(NOT_BEFORE)
        .not_valid_after(NOT_AFTER)
        .add_extension(x509.BasicConstraints(ca=False, path_length=None), critical=True)
        .add_extension(x509.SubjectAlternativeName([x509.DNSName(dns_name)]), critical=False)
        .add_extension(x509.ExtendedKeyUsage([ExtendedKeyUsageOID.SERVER_AUTH]), critical=False)
        .add_extension(
            x509.KeyUsage(
                digital_signature=True, content_commitment=False, key_encipherment=False,
                data_encipherment=False, key_agreement=False, key_cert_sign=False,
                crl_sign=False, encipher_only=False, decipher_only=False,
            ),
            critical=True,
        )
        .add_extension(
            x509.AuthorityKeyIdentifier.from_issuer_public_key(issuer_key.public_key()),
            critical=False,
        )
        .sign(issuer_key, hashes.SHA256())
    )
    return key, cert


def write(file_name, data):
    with open(os.path.join(HERE, file_name), "wb") as handle:
        handle.write(data)


def der(cert):
    return cert.public_bytes(serialization.Encoding.DER)


def pkcs8(key):
    return key.private_bytes(
        serialization.Encoding.DER,
        serialization.PrivateFormat.PKCS8,
        serialization.NoEncryption(),
    )


def main():
    ca_key, ca_cert = ca("DarkBot Test CA")
    _, other_ca_cert = ca("DarkBot Untrusted Test CA")
    localhost_key, localhost_cert = server("localhost", ca_key, ca_cert)
    other_key, other_cert = server("other.test", ca_key, ca_cert)
    write("ca.der", der(ca_cert))
    write("other_ca.der", der(other_ca_cert))
    write("localhost.der", der(localhost_cert))
    write("localhost.key.der", pkcs8(localhost_key))
    write("other_name.der", der(other_cert))
    write("other_name.key.der", pkcs8(other_key))


if __name__ == "__main__":
    main()
