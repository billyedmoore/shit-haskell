module LookupFlagA where
class Value a where value :: a -> Int
instance Value [Int]
