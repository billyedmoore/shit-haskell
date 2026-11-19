-- Derived Functor where the parameter is in a function's argument's argument (covariant again):
-- profunctors' `newtype Baz t b a = Baz (forall f. Applicative f => (a -> f b) -> f t)`.
{-# LANGUAGE DeriveFunctor, RankNTypes #-}
import Data.Functor.Identity (Identity (..))
import Data.Functor.Const (Const (..))

newtype Baz t b a = Baz { runBaz :: forall f. Applicative f => (a -> f b) -> f t }
  deriving Functor

newtype Cont r a = Cont { runCont :: (a -> r) -> r } deriving Functor

data Mixed a = Mixed ((a -> Int) -> String) (Bool -> [a]) deriving Functor

bsell :: a -> Baz b b a
bsell a = Baz (\k -> k a)

main :: IO ()
main = do
  let b = fmap (* 10) (bsell (4 :: Int)) :: Baz Int Int Int
  print (runIdentity (runBaz b (\x -> Identity (x + 1))))
  print (getConst (runBaz b (\x -> Const [x])))
  print (runCont (fmap show (Cont (\k -> k (3 :: Int) ++ "!"))) (++ "?"))
  let Mixed p q = fmap (+ 1) (Mixed (\h -> show (h 2)) (\t -> if t then [1, 2] else [3 :: Int]))
  putStrLn (p (* 5))
  print (q True, q False)
