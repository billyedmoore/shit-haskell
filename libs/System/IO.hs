module System.IO (
    IO, FilePath, Handle, IOMode(..), BufferMode(..),
    stdin, stdout, stderr, hFlush, hPutStr, hPutStrLn, hPrint, hSetBuffering, hGetBuffering,
    hGetLine, hGetContents, hIsEOF, isEOF, hClose, hSetEcho, putStr, putStrLn, print,
    getLine, getContents, getChar, readFile, interact, readLn, putChar
  ) where

import Base

data Handle = Stdin | Stdout | Stderr

instance Eq Handle where
  Stdin == Stdin = True
  Stdout == Stdout = True
  Stderr == Stderr = True
  _ == _ = False

instance Show Handle where
  show Stdin = "<stdin>"
  show Stdout = "<stdout>"
  show Stderr = "<stderr>"

data IOMode = ReadMode | WriteMode | AppendMode | ReadWriteMode
data BufferMode = NoBuffering | LineBuffering | BlockBuffering (Maybe Int)

stdin :: Handle
stdin = Stdin

stdout :: Handle
stdout = Stdout

stderr :: Handle
stderr = Stderr

hFlush :: Handle -> IO ()
hFlush _ = return ()

hPutStr :: Handle -> String -> IO ()
hPutStr Stderr s = IO (primPutErr s)
hPutStr _ s = putStr s

hPutStrLn :: Handle -> String -> IO ()
hPutStrLn h s = hPutStr h (s ++ "\n")

hPrint :: Show a => Handle -> a -> IO ()
hPrint h x = hPutStrLn h (show x)

hSetBuffering :: Handle -> BufferMode -> IO ()
hSetBuffering _ _ = return ()

hGetBuffering :: Handle -> IO BufferMode
hGetBuffering _ = return LineBuffering

hSetEcho :: Handle -> Bool -> IO ()
hSetEcho _ _ = return ()

hGetLine :: Handle -> IO String
hGetLine _ = getLine

hGetContents :: Handle -> IO String
hGetContents _ = getContents

hIsEOF :: Handle -> IO Bool
hIsEOF _ = isEOF

hClose :: Handle -> IO ()
hClose _ = return ()
