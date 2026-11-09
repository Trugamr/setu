package dev.trugamr.setu

import java.io.File
import java.math.BigInteger
import java.security.KeyFactory
import java.security.KeyPairGenerator
import java.security.PrivateKey
import java.security.SecureRandom
import java.security.cert.CertificateFactory
import java.security.cert.X509Certificate
import java.security.spec.PKCS8EncodedKeySpec
import java.util.Date
import org.bouncycastle.asn1.x500.X500Name
import org.bouncycastle.cert.jcajce.JcaX509CertificateConverter
import org.bouncycastle.cert.jcajce.JcaX509v3CertificateBuilder
import org.bouncycastle.operator.jcajce.JcaContentSignerBuilder

/**
 * The agent's adb key: the RSA key the headset approves once during setup, and a self-signed
 * certificate for it, which wireless debugging's TLS presents. adbd checks only the key, so the
 * certificate's names and dates do not matter beyond being valid.
 */
class AdbIdentity(val privateKey: PrivateKey, val certificate: X509Certificate) {

    companion object {
        private const val KEY_FILE = "adbkey.pk8"
        private const val CERTIFICATE_FILE = "adbkey.crt"

        /** Reads the identity kept in [dir], making and saving one the first time. */
        fun loadOrCreate(dir: File): AdbIdentity {
            val keyFile = File(dir, KEY_FILE)
            val certificateFile = File(dir, CERTIFICATE_FILE)
            if (keyFile.exists() && certificateFile.exists()) {
                val key = KeyFactory.getInstance("RSA").generatePrivate(PKCS8EncodedKeySpec(keyFile.readBytes()))
                val certificate = certificateFile.inputStream().use {
                    CertificateFactory.getInstance("X.509").generateCertificate(it) as X509Certificate
                }
                return AdbIdentity(key, certificate)
            }
            val identity = create()
            dir.mkdirs()
            // The certificate last: a key without one is made again, never used half-written.
            keyFile.writeBytes(identity.privateKey.encoded)
            certificateFile.writeBytes(identity.certificate.encoded)
            return identity
        }

        private fun create(): AdbIdentity {
            val pair = KeyPairGenerator.getInstance("RSA").apply { initialize(2048) }.generateKeyPair()
            val name = X500Name("CN=Setu")
            val now = System.currentTimeMillis()
            val builder = JcaX509v3CertificateBuilder(
                name,
                BigInteger(63, SecureRandom()),
                Date(now - DAY_MS),
                Date(now + 100 * YEAR_MS),
                name,
                pair.public,
            )
            val signer = JcaContentSignerBuilder("SHA256withRSA").build(pair.private)
            val certificate = JcaX509CertificateConverter().getCertificate(builder.build(signer))
            return AdbIdentity(pair.private, certificate)
        }

        private const val DAY_MS = 24 * 60 * 60 * 1000L
        private const val YEAR_MS = 365 * DAY_MS
    }
}
