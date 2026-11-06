package dev.trugamr.setu.protocol;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;

/** Reads the commented-hex message fixtures in protocol/fixtures. */
public final class Fixtures {

    private Fixtures() {
    }

    public static byte[] read(String name) throws IOException {
        String dirProperty = System.getProperty("setu.protocolFixtures");
        if (dirProperty == null) {
            throw new IllegalStateException("setu.protocolFixtures is not set; run the tests through Gradle");
        }
        Path dir = Paths.get(dirProperty);
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        for (String line : Files.readAllLines(dir.resolve(name + ".hex"))) {
            int comment = line.indexOf('#');
            String hex = (comment >= 0 ? line.substring(0, comment) : line).trim();
            if (hex.isEmpty()) {
                continue;
            }
            for (String b : hex.split("\\s+")) {
                out.write(Integer.parseInt(b, 16));
            }
        }
        return out.toByteArray();
    }
}
