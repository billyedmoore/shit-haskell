{-# LANGUAGE TypeApplications #-}
import VisibleSpecificityImported (pair)
main :: IO ()
main = print (pair @Bool 'x' True)
