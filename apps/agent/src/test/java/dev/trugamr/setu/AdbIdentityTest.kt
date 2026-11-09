package dev.trugamr.setu

import java.io.File
import java.security.interfaces.RSAPublicKey
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class AdbIdentityTest {

    @get:Rule val temp = TemporaryFolder()

    private val dir: File by lazy { temp.newFolder("adb-identity") }

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

    @Test
    fun encodesItsPublicKeyAsAdbDoes() {
        val key = AdbIdentity.loadOrCreate(dir).certificate.publicKey as RSAPublicKey
        // libadb-android's encoder, which signs in to adbd, is package-private.
        val libadb = Class.forName("io.github.muntashirakon.adb.AndroidPubkey")
            .getDeclaredMethod("encode", RSAPublicKey::class.java)
            .apply { isAccessible = true }
        assertArrayEquals(libadb.invoke(null, key) as ByteArray, AdbIdentity.encode(key))
    }

    @Test
    fun findsItsKeyAmongTheHeadsetsRememberedKeys() {
        val identity = AdbIdentity.loadOrCreate(dir)
        val other = AdbIdentity.loadOrCreate(temp.newFolder("other"))
        fun dumpsys(vararg keys: AdbIdentity) =
            "debugging_manager={\n    user_keys=" + keys.joinToString("\n    ") { "${it.adbPublicKey} Setu agent" } + "\n}"
        assertTrue(identity.isRemembered(dumpsys(other, identity)))
        assertFalse(identity.isRemembered(dumpsys(other)))
    }
}
