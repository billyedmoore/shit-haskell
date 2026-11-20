-- GHC's integer-gmp compatibility package over ghc-bignum (crypton's default flag).
{-# LANGUAGE MagicHash #-}
import GHC.Integer.GMP.Internals (gcdInteger, powModInteger, sizeInBaseInteger, recipModInteger)
import GHC.Exts (Word (W#))
main :: IO ()
main = print (gcdInteger 12 18, powModInteger 3 100 7, W# (sizeInBaseInteger 255 16#), recipModInteger 3 7)
