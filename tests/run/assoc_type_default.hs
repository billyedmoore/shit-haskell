-- A class's default for an associated type applies only to instances without their own equation
-- (hspec-core's `type Arg e = ()` with `instance Example (a -> Result)` setting `Arg = a`).
{-# LANGUAGE TypeFamilies, FlexibleInstances #-}
data Result = Result String deriving Show

class Example e where
  type Arg e
  type Arg e = ()
  run :: e -> Arg e -> Result

instance Example Result where
  run r () = r

instance Example (a -> Result) where
  type Arg (a -> Result) = a
  run f x = f x

instance Example Bool where
  run b () = Result (show b)

main :: IO ()
main = do
  print (run (Result "plain") ())
  print (run (\n -> Result (show (n + 1 :: Int))) 41)
  print (run (\s -> Result (reverse s)) "olleh")
  print (run True ())
