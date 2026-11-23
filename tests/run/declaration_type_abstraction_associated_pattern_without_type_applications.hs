{-# LANGUAGE TypeFamilies,TypeAbstractions,NoTypeApplications #-}
import Data.Kind (Type)
class C a where
  type F (a :: k)
instance C Bool where
  type F @Type Bool = Int
main :: IO ()
main = pure ()
