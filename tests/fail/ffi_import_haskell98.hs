{-# LANGUAGE Haskell98 #-}

import Foreign.C.Types (CInt(..))

foreign import ccall "abs" cAbs :: CInt -> CInt

main = return ()
