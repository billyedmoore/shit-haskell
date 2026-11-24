{-# LANGUAGE TypeAbstractions,NoTypeApplications #-}
import Data.Kind (Type)
data T @((k :: Type) :: Type) a = T
main :: IO ()
main = pure ()
