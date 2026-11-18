{-# LANGUAGE ForeignFunctionInterface #-}
-- C code that sets GHC's RTS flags and keepCAFs links and runs (ghc-lib-parser's cbits, in hlint).
foreign import ccall "rts_flags_touch" touch :: Int -> IO ()
foreign import ccall "rts_flags_keep" keep :: IO Int

main :: IO ()
main = touch (1024 * 1024) >> keep >>= print
