{-# LANGUAGE NoInterruptibleFFI #-}

import Foreign.C.Types (CInt(..))

foreign import ccall safe :: CInt -> IO CInt
foreign import ccall unsafe :: CInt -> IO CInt
foreign import ccall interruptible :: CInt -> IO CInt

main :: IO ()
main = pure ()
