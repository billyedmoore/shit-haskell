{-# LANGUAGE NoInterruptibleFFI #-}

import Foreign.C.Types (CInt(..))

foreign import ccall interruptible "abs" cAbs :: CInt -> IO CInt

main = pure ()
