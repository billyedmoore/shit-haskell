-- StaticPointers: `static e` builds a StaticPtr (hackage-security's trusted functions); the
-- form's info and key are GHC's.
{-# LANGUAGE StaticPointers #-}
import GHC.StaticPtr
import Data.Typeable (Typeable)

newtype Static a = Static (StaticPtr a)

instance IsStatic Static where
  fromStaticPtr = Static

applyS :: Static (a -> b) -> a -> b
applyS (Static p) = deRefStaticPtr p

inc :: Int -> Int
inc = (+ 1)

compose :: StaticPtr (Int -> Int)
compose = static (inc . inc)

main :: IO ()
main = do
  let p = static inc :: StaticPtr (Int -> Int)
  print (deRefStaticPtr p 41, deRefStaticPtr compose 1)
  print (applyS (static show) (3 :: Int))
  print (staticPtrInfo p)
  print (staticPtrInfo compose)
  print (staticKey p == staticKey p, staticKey p == staticKey compose)
  print (deRefStaticPtr (static (\x -> [x, x])) 'a')
