{-# LANGUAGE Haskell98, ForeignFunctionInterface #-}

import Foreign.C.Types (CInt(..))

foreign import ccall "abs" cAbs :: CInt -> CInt

main :: IO ()
main = print (cAbs (-3))
