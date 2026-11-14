package com.codync.android.core

import java.io.ByteArrayOutputStream

internal data class Frame(val counter: Long, val data: ByteArray)

internal class FrameSealer(private val key: ByteArray) {
    private var counter = 0L
    fun seal(message: ByteArray): List<Frame> {
        require(message.size <= RelayCrypto.MAX_OUTBOUND) { "Message is too large" }
        val count = maxOf(1, (message.size + RelayCrypto.CHUNK_SIZE - 1) / RelayCrypto.CHUNK_SIZE)
        require(counter + count <= RelayCrypto.MAX_COUNTER) { "Channel must be rekeyed" }
        return (0 until count).map { index ->
            val from = index * RelayCrypto.CHUNK_SIZE
            val to = minOf(from + RelayCrypto.CHUNK_SIZE, message.size)
            val frame = Frame(counter, RelayCrypto.sealFrame(key, counter, index == count - 1, message.copyOfRange(from, to)))
            counter++
            frame
        }
    }
}

internal class FrameOpener(private val key: ByteArray, private val limit: Int = RelayCrypto.MAX_INBOUND) {
    private var counter = 0L
    private val buffer = ByteArrayOutputStream()
    fun open(frame: Frame): ByteArray? {
        require(frame.counter == counter) { "Encrypted frame is out of order" }
        val (final, chunk) = RelayCrypto.openFrame(key, counter, frame.data)
        require(buffer.size() + chunk.size <= limit) { "Message is too large" }
        counter++
        buffer.write(chunk)
        if (!final) return null
        val message = buffer.toByteArray()
        buffer.reset()
        return message
    }
}

