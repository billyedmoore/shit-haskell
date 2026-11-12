{-# LANGUAGE MagicHash, DeriveGeneric, FlexibleContexts, TypeOperators #-}
import GHC.Generics
import GHC.Exts
data T = T Int# Char# Double# | U Word# deriving Generic
class GS f where gs :: f p -> String
instance GS UInt where gs (UInt i) = show (I# i)
instance GS UChar where gs (UChar c) = show (C# c)
instance GS UDouble where gs (UDouble d) = show (D# d)
instance GS UWord where gs (UWord w) = show (W# w)
instance (GS f, GS g) => GS (f :*: g) where gs (a :*: b) = gs a ++ " " ++ gs b
instance (GS f, GS g) => GS (f :+: g) where { gs (L1 x) = gs x; gs (R1 x) = gs x }
instance GS f => GS (M1 i c f) where gs (M1 x) = gs x
back :: T -> String
back t = case to (from t) of T i c d -> show (I# i, C# c, D# d); U w -> show (W# w)
main = do
  putStrLn (gs (from (T 3# 'x'# 2.5##)))
  putStrLn (gs (from (U 7##)))
  putStrLn (back (T 1# 'y'# 0.5##))
