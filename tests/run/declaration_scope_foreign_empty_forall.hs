{-# LANGUAGE ExplicitForAll, ForeignFunctionInterface #-}
import Foreign.C.Types (CInt(..))

foreign import ccall "abs" cAbs :: forall . CInt -> CInt

main :: IO ()
main = print (cAbs (-3))
