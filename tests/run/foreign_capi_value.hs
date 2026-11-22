{-# LANGUAGE CApiFFI #-}

import Foreign.C.Types (CInt(..))

foreign import capi "unistd.h value STDIN_FILENO" stdinFd :: IO CInt

main :: IO ()
main = stdinFd >>= print
