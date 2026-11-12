{-# LANGUAGE TypeFamilies, FlexibleContexts, UndecidableInstances, FlexibleInstances, ScopedTypeVariables #-}
import Data.Proxy

-- An instance variable that only the context mentions, fixed by an equality.
class Describe a where describe :: a -> String
newtype Box a = Box a
instance (b ~ [a], Show b) => Describe (Box a) where
  describe (Box x) = show ([x, x] :: b)

-- Through a type family, and used in the method via a scoped variable.
type family Elem c
type instance Elem [e] = e
class Firsts c where firsts :: c -> String
instance (e ~ Elem [x], Show e) => Firsts [x] where
  firsts xs = show (take 2 xs :: [e])

main :: IO ()
main = do
  putStrLn (describe (Box 'q'))
  putStrLn (describe (Box (3 :: Int)))
  putStrLn (firsts "hello")
  putStrLn (firsts [True, False, True])
