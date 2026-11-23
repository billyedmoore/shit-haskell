{-# LANGUAGE TypeApplications #-}
import VisibleSpecificityImported (inferred)
main :: IO ()
main = print (inferred @Int 3)
