{-# LANGUAGE FlexibleContexts, TypeOperators #-}
import GHC.Generics
class GC f where gc :: f p -> Int
instance GC U1 where gc _ = 0
instance GC (K1 i c) where gc _ = 1
instance (GC f, GC g) => GC (f :+: g) where { gc (L1 x) = gc x; gc (R1 x) = gc x }
instance (GC f, GC g) => GC (f :*: g) where gc (a :*: b) = gc a + gc b
instance GC f => GC (M1 i c f) where gc (M1 x) = gc x
count :: (Generic a, GC (Rep a)) => a -> Int
count = gc . from
main1 = print (count [1,2,3::Int], count (Just 'x'), count (1::Int, 'c'), count ([] :: [Int]), datatypeName (from "ab"), datatypeName (from (1::Int,2::Int)), moduleName (from "x"))

main2 = do
  case from "ab" of M1 (R1 c) -> print (conName c, conFixity c, conIsRecord c)
  case from "" of M1 (L1 c) -> print (conName c, conFixity c)
  case from ('a', True) of M1 c -> print (conName c, conFixity c)
  case from () of M1 c -> print (conName c, datatypeName (from ()))

main :: IO ()
main = main1 >> main2
