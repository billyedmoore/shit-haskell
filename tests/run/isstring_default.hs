-- OverloadedStrings: a type only IsString and standard classes constrain defaults to String, as GHC
-- defaults it (a splice's `putQ "text"`).
{-# LANGUAGE OverloadedStrings #-}
import Data.String (IsString)
import Data.Typeable
f :: (Show a, IsString a) => a -> String
f = show
g :: (Typeable a, IsString a) => a -> String
g x = show (typeOf x)
h :: (Eq a, IsString a) => a -> Bool
h x = x == x

main :: IO ()
main = putStrLn (f "x") >> putStrLn (g "y") >> print (h "z")
