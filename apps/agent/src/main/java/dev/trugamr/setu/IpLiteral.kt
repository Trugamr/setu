package dev.trugamr.setu

import java.net.InetAddress

/**
 * Parses an IPv4 or IPv6 address literal; null for anything else, including host names and
 * scoped IPv6 addresses. Never looks a name up, so it is safe on the main thread.
 */
fun parseIpLiteral(text: String): InetAddress? {
    val bytes = if (':' in text) ipv6(text) else ipv4(text)
    return bytes?.let(InetAddress::getByAddress)
}

private val IPV4_PART = Regex("""0|[1-9]\d{0,2}""")
private val IPV6_GROUP = Regex("""[0-9a-fA-F]{1,4}""")

private fun ipv4(text: String): ByteArray? {
    val parts = text.split('.')
    if (parts.size != 4 || parts.any { !IPV4_PART.matches(it) || it.toInt() > 255 }) return null
    return ByteArray(4) { parts[it].toInt().toByte() }
}

private fun ipv6(text: String): ByteArray? {
    // A dotted IPv4 tail, as in ::ffff:192.168.1.20, stands for the last two groups.
    val lastColon = text.lastIndexOf(':')
    val tail = text.substring(lastColon + 1)
    val hex = if ('.' in tail) {
        val v4 = ipv4(tail) ?: return null
        fun group(i: Int) = ((v4[i].toInt() and 0xff) shl 8 or (v4[i + 1].toInt() and 0xff)).toString(16)
        text.substring(0, lastColon + 1) + group(0) + ":" + group(2)
    } else {
        text
    }
    val gap = hex.indexOf("::")
    val groups = if (gap < 0) {
        hex.split(':').takeIf { it.size == 8 } ?: return null
    } else {
        if (hex.indexOf("::", gap + 1) >= 0) return null
        val head = groupsOf(hex.substring(0, gap))
        val rest = groupsOf(hex.substring(gap + 2))
        if (head.size + rest.size > 7) return null
        head + List(8 - head.size - rest.size) { "0" } + rest
    }
    if (groups.any { !IPV6_GROUP.matches(it) }) return null
    return ByteArray(16) { i -> (groups[i / 2].toInt(16) shr (if (i % 2 == 0) 8 else 0)).toByte() }
}

private fun groupsOf(text: String) = if (text.isEmpty()) emptyList() else text.split(':')
