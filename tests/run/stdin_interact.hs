main :: IO ()
main = interact $ \s -> unlines (map (show . (* 2) . read) (lines s))
