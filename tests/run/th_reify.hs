{-# LANGUAGE TemplateHaskell #-}
-- `reify` in splices: types declared earlier in the module (for declaration splices) and types
-- of other modules (quoted by name).
module Main where

import Reflect

data Shape = Circle Double | Rect Double Double | Dot deriving Show

data Person = Person { name :: String, age :: Int } deriving Show

mkGetters ''Person

shapeNames :: [String]
shapeNames = $(conNames ''Shape)

main :: IO ()
main = do
  print shapeNames
  print ($(conNames ''Bool), $(conNames ''Maybe), $(conNames ''Ordering))
  let p = Person "Ada" 36
  print (get_name p, get_age p)
  print ($(arity 'Rect), $(arity 'Just), $(arity 'Circle))
