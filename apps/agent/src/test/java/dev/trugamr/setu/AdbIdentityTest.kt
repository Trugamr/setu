package dev.trugamr.setu

import java.io.File
import java.nio.file.Files
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test

class AdbIdentityTest {

    private val dir: File = Files.createTempDirectory("adb-identity").toFile()

    @Test
    fun keepsTheSameKeyAcrossLoads() {
        val made = AdbIdentity.loadOrCreate(dir)
        val loaded = AdbIdentity.loadOrCreate(dir)
        assertArrayEquals(made.privateKey.encoded, loaded.privateKey.encoded)
        assertEquals(made.certificate, loaded.certificate)
    }

    @Test
    fun certifiesItsOwnKey() {
        val identity = AdbIdentity.loadOrCreate(dir)
        identity.certificate.checkValidity()
        // Self-signed: the certificate's own public key verifies its signature.
        identity.certificate.verify(identity.certificate.publicKey)
        assertEquals("RSA", identity.certificate.publicKey.algorithm)
    }

    @Test
    fun aKeyWithoutItsCertificateIsMadeAgain() {
        val first = AdbIdentity.loadOrCreate(dir)
        File(dir, "adbkey.crt").delete()
        val second = AdbIdentity.loadOrCreate(dir)
        assertNotEquals(first.certificate.publicKey, second.certificate.publicKey)
    }
}
